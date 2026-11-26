use std::{
    cell::Cell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    time::Duration,
};

use blit::{
    Animation, AnimationOptions, Constraints, Contact, ContactId, ContactSource, Context, Corners, Easing, FrameInfo,
    Id, Interact, InteractCx, Kernel, Layout, LayoutCx, Motion,
    Phase::{Cancelled, Ended, Held, Started},
    Place, PlaceCx, Point, Rect, Scope, Size, Spring, Tween, Widget,
    input::{
        Input, Modifiers, MouseButton, ScrollPhase, TouchPhase,
        TouchPhase::{Cancel, Down, Move, Up},
    },
};

mod animation;
mod bump;
mod interact;
mod kernel;

type Ui<'a, S = blit::state::Build> = blit::Ui<'a, TestContext, S>;

struct TestContext {
    info: FrameInfo<f32>,
    commands: Vec<Command>,
    prepared: usize,
    duplicates: Vec<u32>,
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

    fn duplicate_key(&mut self, _: Scope, _: u64, location: &'static std::panic::Location<'static>) {
        self.duplicates.push(location.line());
    }

    fn push_clip(&mut self, area: Rect<f32>, _: Corners<f32>) {
        self.commands.push(PushClip(area));
    }

    fn pop_clip(&mut self) {
        self.commands.push(PopClip);
    }
}

/// reports up to two contacts this widget may use, capturing those its predicate accepts
#[derive(Clone, Copy)]
struct Watch(fn(&Contact<'_, f32>) -> bool);

const WATCH: Watch = Watch(|_| false);
const CAPTURE: Watch = Watch(|_| true);

#[derive(Clone, Copy, Debug, PartialEq)]
struct SeenContact {
    id: ContactId,
    source: ContactSource,
    phase: blit::Phase,
    delta: Point<f32>,
}

impl<C: Context<Scalar = f32>> Interact<C> for Watch {
    type State = ();
    /// each contact with whether this update captured it
    type Response = [Option<(SeenContact, bool)>; 2];

    fn update(&self, _: &mut (), cx: &mut InteractCx<'_, f32>) -> Self::Response {
        let mut seen = [None; 2];
        for (seen, mut contact) in seen.iter_mut().zip(cx.contacts().filter(|contact| contact.free)) {
            let captured = (self.0)(&contact) && contact.capture();
            *seen = Some((
                SeenContact {
                    id: contact.id,
                    source: contact.source,
                    phase: contact.phase,
                    delta: contact.delta,
                },
                captured,
            ));
        }
        seen
    }
}

fn kernel(size: Size<f32>) -> (Kernel<TestContext>, TestContext) {
    (
        Kernel::default(),
        TestContext {
            info: FrameInfo::new(size),
            commands: Vec::new(),
            prepared: 0,
            duplicates: Vec::new(),
        },
    )
}

fn render<O>(kernel: &mut Kernel<TestContext>, context: &mut TestContext, build: impl FnMut(Ui<'_>) -> O) -> O {
    render_inputs(kernel, context, Duration::ZERO, [Input::None], build)
}

fn render_inputs<O>(
    kernel: &mut Kernel<TestContext>,
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
        let output = kernel.build(context, info, time, input, &mut build);
        kernel.layout(context);
        match inputs.next() {
            Some(next) => input = next,
            None => break output,
        }
    };
    kernel.paint(context);
    output
}

struct Fill(char, Size<f32>);

impl Widget<TestContext, blit::state::Insert> for Fill {
    type Response = ();

    fn build(self, mut ui: Ui<'_, blit::state::Insert>) {
        ui.measure(move |_, bounds| bounds.constrain(self.1));
        ui.paint(move |cx, area| {
            cx.context_mut().commands.push(Command::Paint(self.0, area));
        });
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

/// positions from the target size under unbounded constraints
struct At(fn(Size<f32>) -> Point<f32>);

impl Place<TestContext> for At {
    fn place(&self, cx: &mut PlaceCx<'_, TestContext>, target: Rect<f32>) -> Point<f32> {
        cx.resolve(Constraints::loose(Size::uniform(f32::INFINITY)));
        let offset = (self.0)(target.size());
        Point::new(target.x + offset.x, target.y + offset.y)
    }
}

/// fills the visual parent
struct Full(fn(Size<f32>) -> Point<f32>);

impl Place<TestContext> for Full {
    fn place(&self, cx: &mut PlaceCx<'_, TestContext>, target: Rect<f32>) -> Point<f32> {
        cx.resolve(Constraints::tight(cx.parent()));
        let offset = (self.0)(target.size());
        Point::new(target.x + offset.x, target.y + offset.y)
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

impl Widget<TestContext, blit::state::Insert> for Content {
    type Response = ();
    fn build(self, mut ui: Ui<'_, blit::state::Insert>) {
        for glyph in self.0.chars() {
            ui.context_mut().prepared += 1;
            ui.insert(Fill(glyph, self.1));
        }
    }
}

struct Probe {
    result: Rc<Cell<ProbeResult>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ProbeResult {
    paints: usize,
    drops: usize,
    area: Rect<f32>,
}

impl Layout<TestContext> for Probe {
    type Item = ();
    fn layout(&self, _: &mut LayoutCx<'_, TestContext>, bounds: Constraints<f32>) -> Size<f32> {
        bounds.min
    }
}

impl Widget<TestContext, blit::state::Insert> for Probe {
    type Response = ();

    fn build(self, mut ui: Ui<'_, blit::state::Insert>) {
        ui.measure(|_, bounds| bounds.constrain(Size::uniform(1.0)));
        ui.paint(move |cx, area| {
            let mut result = self.result.get();
            result.paints += 1;
            result.area = area;
            self.result.set(result);
            cx.context_mut().commands.push(Command::Paint('P', area));
        });
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
        let mut scratch = cx.scratch().alloc(1);
        scratch.push(marker);
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

fn pointer(id: Option<u64>, phase: TouchPhase, position: [f32; 2]) -> Input<f32> {
    let position = Point::new(position[0], position[1]);
    match (id, phase) {
        (Some(id), phase) => Input::Touch { id, position, phase },
        (None, Down) => Input::MouseDown {
            position,
            button: MouseButton::Primary,
            modifiers: Modifiers::NONE,
        },
        (None, Move) => Input::MouseMove {
            position,
            modifiers: Modifiers::NONE,
        },
        (None, Up) => Input::MouseUp {
            position,
            button: MouseButton::Primary,
            modifiers: Modifiers::NONE,
            leave: false,
        },
        (None, Cancel) => Input::Cancel,
    }
}
