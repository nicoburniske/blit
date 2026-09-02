/// state of a node passed to [`Widget::build`]
pub struct Build;

/// state of a node with an active layout
pub struct Open<L> {
    marker: PhantomData<L>,
}

/// state of a child node waiting to be populated
///
/// `P` is the parent layout and `I` tracks its required item
pub struct Pending<P, I = ()> {
    place: Place,
    item: I,
    marker: PhantomData<P>,
}

/// public node states
pub mod node_state {
    pub use super::{Build, Open, Pending};
}

/// a frame node
///
/// [`Build`] nodes are passed to widgets, [`Open`] nodes manage children, and
/// [`Pending`] nodes are configured before receiving a widget or layout
pub struct Node<'ui, R: Platform, S = Build> {
    ui: &'ui mut Ui<R>,
    node: NodeId,
    state: S,
    open: bool,
}

impl<'ui, R: Platform, S> Node<'ui, R, S> {
    pub fn id(&self) -> NodeId {
        self.node
    }

    pub fn clip<C: Clip<R>>(self, clip: C) -> Self {
        let node = self.node;
        let frame = self.ui.frame_mut();
        assert!(
            frame.nodes[node.index()].clip.index().is_none(),
            "node already has a clip"
        );
        let clip = frame.store_clip(clip);
        frame.nodes[node.index()].clip = clip;
        self
    }

    pub fn absolute(self, absolute: Absolute) -> Self {
        let node = self.node;
        self.ui.frame_mut().set_absolute(node, absolute);
        self
    }

    pub fn widget_id(self, id: WidgetId) -> Self {
        let node = self.node;
        self.ui.frame_mut().set_id(node, id);
        self
    }

    pub fn hit(self, hit: Sides) -> Self {
        let node = self.node;
        self.ui.frame_mut().set_hit(node, hit);
        self
    }

    pub fn transition(self, transition: Transition) -> Self {
        let node = self.node;
        self.ui
            .frame_mut()
            .set_transition(node, transition);
        self
    }

    fn new(ui: &'ui mut Ui<R>, node: NodeId, state: S, open: bool) -> Self {
        Self {
            ui,
            node,
            state,
            open,
        }
    }

    fn into_parts(self) -> (&'ui mut Ui<R>, NodeId, S) {
        let node = self.node;
        let this = ManuallyDrop::new(self);
        // safety: this is not dropped and each field is read once
        unsafe { (ptr::read(&this.ui), node, ptr::read(&this.state)) }
    }
}

impl<'ui, R: Platform> Node<'ui, R, Build> {
    /// appends an atom to the node
    pub fn atom<A: Atom<R>>(&mut self, atom: A) -> &mut Self {
        let node = self.node;
        self.ui.frame_mut().push_atom(node, atom);
        self
    }

    /// establishes the node's layout
    pub fn layout<L: Layout<R>>(self, layout: L) -> Node<'ui, R, Open<L>> {
        let (ui, node, _) = self.into_parts();
        let frame = ui.frame_mut();
        assert!(
            frame.nodes[node.index()].layout.index().is_none(),
            "node already has a layout"
        );
        let layout = frame.store_layout(layout);
        frame.nodes[node.index()].layout = layout;
        open_node(ui, node)
    }

    pub fn new_layer(&mut self) -> LayerId {
        self.ui.frame_mut().add_layer()
    }
}

impl<'ui, R: Platform, L: Layout<R>> Node<'ui, R, Open<L>> {
    pub fn offset(self, offset: Point) -> Self {
        let node = self.node;
        let frame = self.ui.frame_mut();
        let layout = frame.nodes[node.index()].layout.index().unwrap();
        frame.layouts[layout].offset = offset;
        self
    }

    /// inserts a widget into the node
    pub fn insert<W: Widget<R>>(&mut self, widget: W) -> W::Response {
        let node = self.node;
        widget.build(Node::new(&mut *self.ui, node, Build, false))
    }

    pub fn new_layer(&mut self) -> LayerId {
        self.ui.frame_mut().add_layer()
    }

    /// creates a child node
    #[must_use = "a child node must be populated with insert or layout"]
    pub fn child(&mut self) -> Node<'_, R, Pending<L>> {
        let node = self.ui.frame_mut().push_node(None);
        Node::new(
            &mut *self.ui,
            node,
            Pending {
                place: Place::new(),
                item: (),
                marker: PhantomData,
            },
            false,
        )
    }
}

impl<'ui, R: Platform, L: Layout<R, Item = ()>> Node<'ui, R, Open<L>> {
    /// adds a child widget with default placement
    pub fn add<W: Widget<R>>(&mut self, widget: W) -> W::Response {
        self.child().insert(widget)
    }
}

impl<'ui, R: Platform, P: Layout<R>, I> Node<'ui, R, Pending<P, I>> {
    pub fn item(self, item: P::Item) -> Node<'ui, R, Pending<P, P::Item>> {
        let (ui, node, state) = self.into_parts();
        Node::new(
            ui,
            node,
            Pending {
                place: state.place,
                item,
                marker: PhantomData,
            },
            false,
        )
    }

    pub fn place(mut self, place: Place) -> Self {
        self.state.place = place;
        self
    }
}

impl<'ui, R: Platform, P: Layout<R>> Node<'ui, R, Pending<P, P::Item>> {
    /// inserts a widget into the node
    pub fn insert<W: Widget<R>>(self, widget: W) -> W::Response {
        let (ui, node, state) = self.into_parts();
        let response = ui.build_node(node, widget);
        let frame = ui.frame_mut();
        frame.set_place(node, state.place);
        let item = frame.data.store(state.item);
        frame.nodes[node.index()].item = item;
        response
    }

    /// establishes the node's layout
    pub fn layout<L: Layout<R>>(self, layout: L) -> Node<'ui, R, Open<L>> {
        let (ui, node, state) = self.into_parts();
        let frame = ui.frame_mut();
        let layout = frame.store_layout(layout);
        frame.nodes[node.index()].layout = layout;
        frame.set_place(node, state.place);
        let item = frame.data.store(state.item);
        frame.nodes[node.index()].item = item;
        open_node(ui, node)
    }
}

impl<R: Platform, S> Node<'_, R, S> {
    pub fn geometry(&self, id: WidgetId) -> Option<Rect> {
        self.ui.geometry(id)
    }

    pub fn interact(&mut self, id: WidgetId, sense: Sense) -> Interaction {
        self.ui.interact(id, sense)
    }

    pub fn input(&self) -> &Input {
        self.ui.input()
    }

    /// accesses platform resources during frame construction
    ///
    /// drawing remains deferred to [`Atom`] implementations
    pub fn platform(&mut self) -> &mut R {
        self.ui.platform()
    }

    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.ui.is_focused(id)
    }

    pub fn focus(&mut self, id: WidgetId) {
        self.ui.focus(id);
    }

    pub fn clear_focus(&mut self) {
        self.ui.clear_focus();
    }

    pub fn pointer_position(&self) -> Option<Point> {
        self.ui.pointer_position()
    }

    pub fn screen(&self) -> Rect {
        self.ui.screen()
    }

    pub fn time(&self) -> Duration {
        self.ui.time()
    }

    pub fn animate(
        &mut self,
        id: WidgetId,
        target: f32,
        duration: Duration,
        easing: Easing,
    ) -> f32 {
        self.ui.animate(id, target, duration, easing)
    }

    pub fn animate_loop(&mut self, id: WidgetId, duration: Duration, easing: Easing) -> f32 {
        self.ui.animate_loop(id, duration, easing)
    }

    pub fn timer(&mut self, id: WidgetId, duration: Duration) -> bool {
        self.ui.timer(id, duration)
    }

    pub fn timer_loop(&mut self, id: WidgetId, duration: Duration) -> bool {
        self.ui.timer_loop(id, duration)
    }

    pub fn request_frame(&mut self) {
        self.ui.request_frame();
    }
}

impl<R: Platform, S> Drop for Node<'_, R, S> {
    fn drop(&mut self) {
        if !self.open {
            return;
        }
        let node = self.node;
        let frame = self.ui.frame_mut();
        frame.nodes[node.index()].subtree_end =
            u32::try_from(frame.nodes.len() - 1).expect("too many frame nodes");
        let parent = frame.nodes[node.index()].parent;
        frame.current_parent = (parent != node).then_some(parent);
    }
}

fn open_node<R: Platform, L: Layout<R>>(
    ui: &mut Ui<R>,
    node: NodeId,
) -> Node<'_, R, Open<L>> {
    ui.frame_mut().current_parent = Some(node);
    Node::new(
        ui,
        node,
        Open {
            marker: PhantomData,
        },
        true,
    )
}

/// frame-local paint layer
///
/// do not store this across renders
#[cfg_attr(not(debug_assertions), repr(transparent))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerId(NonZeroU16, #[cfg(debug_assertions)] u16);

crate::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Place {
        new(),
        @optional {
            layer: LayerId,
        },
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
        z_index: i16 = 0,
    }
}

impl Place {
    pub const fn fixed(width: f32, height: f32) -> Self {
        Self {
            layer: None,
            width: Sizing::fixed(width),
            height: Sizing::fixed(height),
            z_index: 0,
        }
    }

    pub const fn grow() -> Self {
        Self {
            layer: None,
            width: Sizing::grow(),
            height: Sizing::grow(),
            z_index: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    Fit { min: f32, max: f32 },
    Grow { min: f32, max: f32 },
    Fixed(f32),
    Percent(f32),
}

impl Sizing {
    pub const fn fit() -> Self {
        Self::Fit {
            min: 0.0,
            max: f32::INFINITY,
        }
    }

    pub const fn grow() -> Self {
        Self::Grow {
            min: 0.0,
            max: f32::INFINITY,
        }
    }

    pub const fn fixed(size: f32) -> Self {
        Self::Fixed(size)
    }

    pub const fn percent(fraction: f32) -> Self {
        Self::Percent(fraction)
    }

    pub const fn min(self, value: f32) -> Self {
        match self {
            Self::Fit { max, .. } => Self::Fit { min: value, max },
            Self::Grow { max, .. } => Self::Grow { min: value, max },
            Self::Fixed(_) | Self::Percent(_) => self,
        }
    }

    pub const fn max(self, value: f32) -> Self {
        match self {
            Self::Fit { min, .. } => Self::Fit { min, max: value },
            Self::Grow { min, .. } => Self::Grow { min, max: value },
            Self::Fixed(_) | Self::Percent(_) => self,
        }
    }

    #[inline]
    pub fn resolve(self, intrinsic: f32, available: f32, cross: bool) -> f32 {
        match self {
            Self::Fit { .. } => self.clamp(intrinsic.min(available)),
            Self::Grow { .. } if cross => self.clamp(available),
            Self::Grow { .. } => self.clamp(intrinsic.min(available)),
            Self::Fixed(size) => size.max(0.0),
            Self::Percent(fraction) if available.is_finite() => {
                assert!((0.0..=1.0).contains(&fraction));
                available * fraction
            }
            Self::Percent(_) => 0.0,
        }
    }

    #[inline]
    pub fn clamp(self, size: f32) -> f32 {
        match self {
            Self::Fit { min, max } | Self::Grow { min, max } => {
                size.clamp(min.max(0.0), max.max(min).max(0.0))
            }
            Self::Fixed(fixed) => fixed.max(0.0),
            Self::Percent(_) => size.max(0.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Absolute {
    pub target: PositionTarget,
    pub target_anchor: Anchor,
    pub child_anchor: Anchor,
    pub offset: Point,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PositionTarget {
    #[default]
    Parent,
    Node(NodeId),
    Screen,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Absolute {
    pub const fn at(x: f32, y: f32) -> Self {
        Self {
            target: PositionTarget::Parent,
            target_anchor: Anchor::TopLeft,
            child_anchor: Anchor::TopLeft,
            offset: Point::new(x, y),
        }
    }

    pub const fn screen(x: f32, y: f32) -> Self {
        Self {
            target: PositionTarget::Screen,
            ..Self::at(x, y)
        }
    }

    pub const fn attach(target: Anchor, child: Anchor) -> Self {
        Self::at(0.0, 0.0).anchors(target, child)
    }

    pub const fn relative_to(mut self, target: NodeId) -> Self {
        self.target = PositionTarget::Node(target);
        self
    }

    pub const fn anchors(mut self, target: Anchor, child: Anchor) -> Self {
        self.target_anchor = target;
        self.child_anchor = child;
        self
    }

    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = Point::new(x, y);
        self
    }
}

fn layer_id(index: usize) -> LayerId {
    let value = u16::try_from(index + 1).expect("too many layers in one frame");
    LayerId(
        NonZeroU16::new(value).unwrap(),
        #[cfg(debug_assertions)]
        generation::get(),
    )
}

fn layer_index(id: LayerId) -> usize {
    #[cfg(debug_assertions)]
    generation::assert(id.1);
    id.0.get() as usize - 1
}

fn layer_order(id: LayerId) -> u16 {
    #[cfg(debug_assertions)]
    generation::assert(id.1);
    id.0.get()
}
