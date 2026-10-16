use std::{
    cell::Cell,
    f32::consts::FRAC_PI_4,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    time::Duration,
};

use blit::{
    Atom, AtomCx, Clip, Constraints, Context, Direction, Easing, Frame, FrameInfo, Id, Input, Interaction, Layout,
    LayoutCx, Modifiers, NodeTarget,
    Phase::{Cancelled, Changed, Ended, Started},
    Point, PointerButton, Rect, Scalar, ScrollPhase, Sense, Size, TouchPhase,
    TouchPhase::{Cancel, Down, Move, Up},
    Transition, Widget,
};

mod animation;
mod frame;
mod interaction;

type Ui<'a, S = blit::state::Build> = blit::Ui<'a, TestContext, S>;

struct TestContext {
    info: FrameInfo<f32>,
    commands: Vec<Command>,
    prepared: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Command {
    Paint(char, Rect<f32>),
    PushClip(Rect<f32>),
    PopClip,
}

use Command::{Paint, PopClip, PushClip};

impl Context for TestContext {
    type Scalar = f32;
}

fn frame(size: Size<f32>) -> (Frame<TestContext>, TestContext) {
    (
        Frame::default(),
        TestContext {
            info: FrameInfo::new(size),
            commands: Vec::new(),
            prepared: 0,
        },
    )
}

fn render<O>(frame: &mut Frame<TestContext>, context: &mut TestContext, build: impl FnMut(Ui<'_>) -> O) -> O {
    render_inputs(frame, context, Duration::ZERO, [Input::None], build)
}

fn render_inputs<O>(
    frame: &mut Frame<TestContext>,
    context: &mut TestContext,
    time: Duration,
    inputs: impl IntoIterator<Item = Input<f32>>,
    mut build: impl FnMut(Ui<'_>) -> O,
) -> O {
    let info = context.info;
    context.commands.clear();
    let mut inputs = inputs.into_iter();
    let mut input = inputs.next().unwrap_or(Input::None);
    let output = loop {
        let output = frame.build(context, info, time, input, &mut build);
        frame.layout(context);
        match inputs.next() {
            Some(next) => input = next,
            None => break output,
        }
    };
    frame.paint(context);
    output
}

struct Fill(char, Size<f32>);

blit::atom_widget!(Fill);

impl Atom<TestContext> for Fill {
    fn measure(&self, _: &mut AtomCx<'_, TestContext>, bounds: Constraints<f32>) -> Size<f32> {
        bounds.constrain(self.1)
    }

    fn paint(&self, cx: &mut AtomCx<'_, TestContext>, area: Rect<f32>) {
        cx.context_mut().commands.push(Command::Paint(self.0, area));
    }
}

struct TestClip;

impl Clip<TestContext> for TestClip {
    fn push(&self, context: &mut TestContext, area: Rect<f32>) {
        context.commands.push(Command::PushClip(area));
    }
    fn pop(&self, context: &mut TestContext) {
        context.commands.push(Command::PopClip);
    }
}

struct Overlay;

impl Layout<TestContext> for Overlay {
    type Item = ();

    fn layout(&self, cx: &mut LayoutCx<'_, TestContext>, bounds: Constraints<f32>) -> Size<f32> {
        let mut size: Size<f32> = Size::ZERO;
        for child in cx.children() {
            size = size.max(cx.layout_child(child, Constraints::loose(bounds.max)));
        }
        let size = bounds.constrain(size);
        for child in cx.children() {
            let child_size = cx.size(child);
            cx.set_child_position(
                child,
                Point::new(
                    (size.width - child_size.width) / 2.0,
                    (size.height - child_size.height) / 2.0,
                ),
            );
        }
        size
    }
}

struct Column;

impl Layout<TestContext> for Column {
    type Item = f32;

    fn layout(&self, cx: &mut LayoutCx<'_, TestContext, f32>, bounds: Constraints<f32>) -> Size<f32> {
        let mut size: Size<f32> = Size::ZERO;
        for child in cx.children() {
            let child_size = cx.layout_child(child, Constraints::loose(bounds.max));
            size.width = size.width.max(child_size.width);
            size.height += cx.item(child);
            cx.set_child_position(child, Point::new(0.0, size.height));
            size.height += child_size.height;
        }
        bounds.constrain(size)
    }
}

struct Fixed(Size<f32>);

impl Layout<TestContext> for Fixed {
    type Item = ();

    fn layout(&self, cx: &mut LayoutCx<'_, TestContext>, bounds: Constraints<f32>) -> Size<f32> {
        for child in cx.children() {
            cx.layout_child(child, Constraints::loose(bounds.max));
            cx.set_child_position(child, Point::ZERO);
        }
        bounds.constrain(self.0)
    }
}

struct At<L>(L, fn(Size<f32>) -> Point<f32>);

impl<L: Layout<TestContext>> Layout<TestContext> for At<L> {
    type Item = L::Item;
    fn on_insert<'a>(&self, ui: Ui<'a>) -> Ui<'a> {
        ui.relative(NodeTarget::Parent)
    }
    fn layout(&self, cx: &mut LayoutCx<'_, TestContext, Self::Item>, bounds: Constraints<f32>) -> Size<f32> {
        let size = self.0.layout(cx, bounds);
        cx.set_position((self.1)(cx.size(cx.relative())));
        size
    }
}

struct Full;

impl Layout<TestContext> for Full {
    type Item = ();
    fn layout(&self, cx: &mut LayoutCx<'_, TestContext>, _: Constraints<f32>) -> Size<f32> {
        ().layout(cx, Constraints::tight(cx.size(cx.parent())))
    }
}

struct Rects;

impl Layout<TestContext> for Rects {
    type Item = Rect<f32>;
    fn layout(&self, cx: &mut LayoutCx<'_, TestContext, Rect<f32>>, bounds: Constraints<f32>) -> Size<f32> {
        for child in cx.children() {
            let rect = *cx.item(child);
            cx.layout_child(child, Constraints::tight(rect.size()));
            cx.set_child_position(child, Point::new(rect.x, rect.y));
        }
        bounds.min
    }
}

struct Content(&'static str, Size<f32>);

impl Widget<TestContext, blit::state::Node> for Content {
    type Response = ();
    fn build(self, mut ui: Ui<'_, blit::state::Node>) {
        for glyph in self.0.chars() {
            ui.context_mut().prepared += 1;
            ui.insert(Fill(glyph, self.1));
        }
    }
}

struct Probe {
    result: Rc<Cell<ProbeResult>>,
    offset: Point<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ProbeResult {
    paints: usize,
    drops: usize,
    area: Rect<f32>,
}

blit::atom_widget!(Probe);

impl Layout<TestContext> for Probe {
    type Item = ();
    fn layout(&self, _: &mut LayoutCx<'_, TestContext>, bounds: Constraints<f32>) -> Size<f32> {
        bounds.min
    }
}

impl Atom<TestContext> for Probe {
    fn measure(&self, _: &mut AtomCx<'_, TestContext>, bounds: Constraints<f32>) -> Size<f32> {
        bounds.constrain(Size::uniform(1.0))
    }
    fn paint(&self, cx: &mut AtomCx<'_, TestContext>, area: Rect<f32>) {
        let mut result = self.result.get();
        result.paints += 1;
        result.area = area;
        self.result.set(result);
        Fill('P', Size::ZERO).paint(cx, area);
    }
    fn paint_bounds(&self, area: Rect<f32>) -> Rect<f32> {
        Rect::new(area.x + self.offset.x, area.y + self.offset.y, area.width, area.height)
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let mut result = self.result.get();
        result.drops += 1;
        self.result.set(result);
    }
}

struct Temporary;

impl Layout<TestContext> for Temporary {
    type Item = ();
    fn layout(&self, cx: &mut LayoutCx<'_, TestContext>, bounds: Constraints<f32>) -> Size<f32> {
        cx.context_mut().prepared += 1;
        let marker = cx.context().prepared;
        let scratch = cx.scratch(1, marker);
        let size = ().layout(cx, bounds);
        assert_eq!(scratch[0], marker);
        size
    }
}

struct SharedItems(usize);

impl Layout<TestContext> for SharedItems {
    type Item = Rc<()>;
    fn layout(&self, cx: &mut LayoutCx<'_, TestContext, Rc<()>>, bounds: Constraints<f32>) -> Size<f32> {
        assert_eq!(cx.children().count(), self.0);
        let first = Rc::as_ptr(cx.item(cx.children().next().unwrap()));
        for child in cx.children() {
            assert_eq!(Rc::as_ptr(cx.item(child)), first);
            cx.layout_child(child, Constraints::loose(bounds.max));
            cx.set_child_position(child, Point::ZERO);
        }
        bounds.min
    }
}

#[derive(Default)]
struct Integer;
impl Context for Integer {
    type Scalar = i32;
}

type Scene = InputScene<()>;

struct InputScene<C: Context> {
    frame: Frame<C>,
    context: C,
    sense: Sense,
}

impl<C: Context + Default> InputScene<C> {
    fn new(sense: Sense) -> Self {
        let mut scene = Self {
            frame: Frame::default(),
            context: C::default(),
            sense,
        };
        scene.step(0, Input::None);
        scene
    }
    fn step(&mut self, time: u64, input: Input<C::Scalar>) -> Interaction<C::Scalar> {
        let output = self.frame.build(
            &mut self.context,
            FrameInfo::new(Size::uniform(C::Scalar::from_f32(200.0))),
            Duration::from_millis(time),
            input,
            |mut ui: blit::Ui<'_, C>| ui.interact(self.sense),
        );
        self.frame.layout(&mut self.context);
        output
    }
}

fn pointer(id: Option<u64>, phase: TouchPhase, position: [f32; 2]) -> Input<f32> {
    let position = Point::new(position[0], position[1]);
    match (id, phase) {
        (Some(id), phase) => Input::Touch { id, position, phase },
        (None, Down) => Input::PointerDown {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
        (None, Move) => Input::PointerMove {
            position,
            modifiers: Modifiers::NONE,
        },
        (None, Up) => Input::PointerUp {
            position,
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
            leave: false,
        },
        (None, Cancel) => Input::Cancel,
    }
}
