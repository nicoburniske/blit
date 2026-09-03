pub mod animation;
pub mod interaction;
pub mod layout;
pub mod paint;
pub mod position;
pub mod timer;
pub mod transition;

use std::{
    any::TypeId, marker::PhantomData, mem::size_of, num::NonZeroU16, ptr::NonNull, time::Duration,
};

use crate::{
    Atom, Clip, Content, FrameInfo, Platform, Widget,
    animation::{Easing, Transition},
    arena::{DataArena, DataId},
    geometry::{Constraints, Point, Rect, Sides, Size},
    input::Input,
    interact::{Interaction, Sense, WidgetId},
    layout::{Axis, Layout, LayoutResolution},
};

/// typestate modes for [`crate::Ui`]
///
/// every mode can insert content and access shared frame services
pub mod state {
    use super::PhantomData;

    /// an unlaid node that may establish a layout
    pub struct Build;

    /// a laid-out node that may create children
    pub struct Open<L>(PhantomData<L>);

    /// access to an existing node without layout or child creation
    pub struct Node;
}

/// scoped handle for building a frame node
///
/// its [`state`] mode determines which operations are available
pub struct Ui<'ui, R: Platform, S = state::Build> {
    inner: UiInner<'ui, R>,
    marker: PhantomData<S>,
}

impl<'ui, R: Platform, S> Ui<'ui, R, S> {
    pub fn id(&self) -> NodeId {
        self.inner.node
    }

    pub fn clip<C: Clip<R>>(self, clip: C) -> Self {
        let node = self.inner.node;
        let frame = self.inner.context.frame_mut();
        let clip = frame.store_clip(clip);
        frame.nodes[node.index()].clip = clip;
        self
    }

    pub fn widget_id(self, id: WidgetId) -> Self {
        let node = self.inner.node;
        self.inner.context.frame_mut().geometry_mut(node).id = Some(id);
        self
    }

    pub fn hit(self, hit: Sides) -> Self {
        let node = self.inner.node;
        self.inner.context.frame_mut().geometry_mut(node).hit = hit;
        self
    }

    pub fn transition(self, transition: Transition) -> Self {
        let node = self.inner.node;
        self.inner.context.frame_mut().geometry_mut(node).transition = Some(transition);
        self
    }

    /// inserts content into the current node
    pub fn insert<C: Content<R>>(&mut self, content: C) -> C::Response {
        content.append(Ui {
            inner: UiInner {
                context: &mut *self.inner.context,
                node: self.inner.node,
                owns_node: false,
            },
            marker: PhantomData,
        })
    }

    pub fn new_layer(&mut self) -> LayerId {
        self.inner.context.frame_mut().add_layer()
    }
}

impl<'ui, R: Platform> Ui<'ui, R, state::Build> {
    /// transfers this fresh node to a widget
    pub fn build<W: Widget<R>>(self, widget: W) -> W::Response {
        widget.build(self)
    }

    /// establishes the current node's layout
    pub fn layout<L: Layout<R>>(self, layout: L) -> Ui<'ui, R, state::Open<L>> {
        let Ui { inner, .. } = self;
        let frame = inner.context.frame_mut();
        let layout = frame.store_layout(layout);
        frame.nodes[inner.node.index()].layout = layout;
        Ui {
            inner,
            marker: PhantomData,
        }
    }
}

impl<'ui, R: Platform, L: Layout<R>> Ui<'ui, R, state::Open<L>> {
    pub fn offset(self, offset: Point) -> Self {
        let node = self.inner.node;
        let frame = self.inner.context.frame_mut();
        let layout = frame.nodes[node.index()].layout.index().unwrap();
        frame.layouts[layout].offset = offset;
        self
    }

    /// creates a fresh child at the given place
    pub fn child(&mut self, place: Place<L::Item>) -> Ui<'_, R> {
        let Place {
            kind,
            layer,
            width,
            height,
            z_index,
        } = place;
        let node = self.inner.context.frame_mut().push_node();
        let frame = self.inner.context.frame_mut();
        let kind = match kind {
            PlaceKind::Layout(item) => {
                frame.nodes[node.index()].item = frame.data.store(item);
                PlaceKind::Layout(())
            }
            PlaceKind::Absolute(absolute) => {
                frame.set_absolute(node, absolute);
                PlaceKind::Absolute(absolute)
            }
        };
        frame.set_place(
            node,
            Place {
                kind,
                layer,
                width,
                height,
                z_index,
            },
        );
        frame.current_parent = Some(node);
        Ui::new(&mut *self.inner.context, node)
    }
}

impl<R: Platform, S> Ui<'_, R, S> {
    pub fn geometry(&self, id: WidgetId) -> Option<Rect> {
        self.inner.context.frame().geometry(id)
    }

    pub fn interact(&mut self, id: WidgetId, sense: Sense) -> Interaction {
        let frame = self.inner.context.frame_mut();
        let interaction = frame.interaction.response(id, sense);
        if interaction.activated || interaction.deactivated || interaction.clicked {
            frame.request_frame();
        }
        interaction
    }

    pub fn input(&self) -> &Input {
        &self.inner.context.frame().input
    }

    /// accesses platform resources during frame construction
    ///
    /// drawing remains deferred to [`Atom`] implementations
    pub fn platform(&mut self) -> &mut R {
        self.inner.context.platform_mut()
    }

    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.inner.context.frame().interaction.is_focused(id)
    }

    pub fn focus(&mut self, id: WidgetId) {
        let frame = self.inner.context.frame_mut();
        if frame.interaction.focus(id) {
            frame.request_frame();
        }
    }

    pub fn clear_focus(&mut self) {
        let frame = self.inner.context.frame_mut();
        if frame.interaction.clear_focus() {
            frame.request_frame();
        }
    }

    pub fn pointer_position(&self) -> Option<Point> {
        self.inner.context.frame().interaction.pointer_position()
    }

    pub fn screen(&self) -> Rect {
        self.inner.context.frame().screen
    }

    /// resolves an extent using the frame's layout resolution
    pub fn resolve_extent(&self, axis: Axis, value: f32) -> f32 {
        self.inner
            .context
            .frame()
            .layout_resolution
            .extent(axis, value)
    }

    pub fn time(&self) -> Duration {
        self.inner.context.frame().time
    }

    pub fn animate(
        &mut self,
        id: WidgetId,
        target: f32,
        duration: Duration,
        easing: Easing,
    ) -> f32 {
        let frame = self.inner.context.frame_mut();
        let time = frame.time;
        animation::AnimationState::update(&mut frame.animations, id, target, |animation| {
            animation.advance(target, duration, easing, time)
        })
    }

    pub fn animate_loop(&mut self, id: WidgetId, duration: Duration, easing: Easing) -> f32 {
        let frame = self.inner.context.frame_mut();
        let time = frame.time;
        animation::AnimationState::update(&mut frame.animations, id, 0.0, |animation| {
            animation.advance_loop(duration, easing, time)
        })
    }

    pub fn timer(&mut self, id: WidgetId, duration: Duration) -> bool {
        let frame = self.inner.context.frame_mut();
        timer::TimerState::update(&mut frame.timers, id, duration, None, frame.time)
    }

    pub fn timer_loop(&mut self, id: WidgetId, duration: Duration) -> bool {
        assert!(
            !duration.is_zero(),
            "looping timer duration must be nonzero"
        );
        let frame = self.inner.context.frame_mut();
        timer::TimerState::update(&mut frame.timers, id, duration, Some(duration), frame.time)
    }

    pub fn request_frame(&mut self) {
        self.inner.context.frame_mut().request_frame();
    }
}

// all atoms are content
impl<R: Platform, A: Atom<R>> Content<R> for A {
    type Response = ();

    fn append(self, ui: Ui<'_, R, state::Node>) {
        let node = ui.inner.node;
        ui.inner.context.frame_mut().push_atom(node, self);
    }
}

/// identifies a node only during the current render
///
/// do not store this across renders
#[cfg_attr(not(debug_assertions), repr(transparent))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeId {
    value: u32,
    #[cfg(debug_assertions)]
    generation: u16,
}

/// frame-local paint layer
///
/// do not store this across renders
#[cfg_attr(not(debug_assertions), repr(transparent))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerId {
    value: NonZeroU16,
    #[cfg(debug_assertions)]
    generation: u16,
}

/// placement of a child within its parent
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place<I = ()> {
    pub kind: PlaceKind<I>,
    pub layer: Option<LayerId>,
    pub width: Sizing,
    pub height: Sizing,
    pub z_index: i16,
}

/// relationship between a child and its parent layout
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlaceKind<I = ()> {
    Layout(I),
    Absolute(Absolute),
}

impl Place<()> {
    pub const fn new() -> Self {
        Self {
            kind: PlaceKind::Layout(()),
            layer: None,
            width: Sizing::fit(),
            height: Sizing::fit(),
            z_index: 0,
        }
    }

    pub const fn fixed(width: f32, height: f32) -> Self {
        Self {
            kind: PlaceKind::Layout(()),
            layer: None,
            width: Sizing::fixed(width),
            height: Sizing::fixed(height),
            z_index: 0,
        }
    }

    pub const fn grow() -> Self {
        Self {
            kind: PlaceKind::Layout(()),
            layer: None,
            width: Sizing::grow(),
            height: Sizing::grow(),
            z_index: 0,
        }
    }

    pub fn item<I>(item: I) -> Place<I> {
        Place {
            kind: PlaceKind::Layout(item),
            layer: None,
            width: Sizing::fit(),
            height: Sizing::fit(),
            z_index: 0,
        }
    }
}

impl Default for Place<()> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I> Place<I> {
    pub const fn absolute(absolute: Absolute) -> Self {
        Self {
            kind: PlaceKind::Absolute(absolute),
            layer: None,
            width: Sizing::fit(),
            height: Sizing::fit(),
            z_index: 0,
        }
    }

    pub const fn layer(mut self, layer: LayerId) -> Self {
        self.layer = Some(layer);
        self
    }

    pub const fn width(mut self, width: Sizing) -> Self {
        self.width = width;
        self
    }

    pub const fn height(mut self, height: Sizing) -> Self {
        self.height = height;
        self
    }

    pub const fn z_index(mut self, z_index: i16) -> Self {
        self.z_index = z_index;
        self
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

//
// internals
//

include!("graph.rs");

struct UiInner<'ui, R: Platform> {
    context: &'ui mut Context<R>,
    node: NodeId,
    owns_node: bool,
}

impl<'ui, R: Platform> Ui<'ui, R, state::Build> {
    fn new(context: &'ui mut Context<R>, node: NodeId) -> Self {
        Self {
            inner: UiInner {
                context,
                node,
                owns_node: true,
            },
            marker: PhantomData,
        }
    }
}

impl<R: Platform> Drop for UiInner<'_, R> {
    fn drop(&mut self) {
        if !self.owns_node {
            return;
        }
        let node = self.node;
        let frame = self.context.frame_mut();
        frame.nodes[node.index()].subtree_end =
            u32::try_from(frame.nodes.len() - 1).expect("too many frame nodes");
        let parent = frame.nodes[node.index()].parent;
        frame.current_parent = (parent != node).then_some(parent);
    }
}

impl NodeId {
    fn new(index: usize) -> Self {
        Self {
            value: u32::try_from(index).expect("too many frame nodes"),
            #[cfg(debug_assertions)]
            generation: generation::get(),
        }
    }

    fn index(self) -> usize {
        #[cfg(debug_assertions)]
        generation::assert(self.generation);
        self.value as usize
    }
}

impl LayerId {
    fn new(index: usize) -> Self {
        let value = u16::try_from(index + 1).expect("too many layers in one frame");
        Self {
            value: NonZeroU16::new(value).unwrap(),
            #[cfg(debug_assertions)]
            generation: generation::get(),
        }
    }

    fn index(self) -> usize {
        #[cfg(debug_assertions)]
        generation::assert(self.generation);
        self.value.get() as usize - 1
    }

    fn order(self) -> u16 {
        #[cfg(debug_assertions)]
        generation::assert(self.generation);
        self.value.get()
    }
}

struct Context<R: Platform> {
    frame: NonNull<Frame<R>>,
    platform: NonNull<R>,
}

// Context erases frame and platform lifetimes while a frame is built
// Frame::record keeps the only value inside the build callback
// NonNull is dereferenced only through Ui borrows
impl<R: Platform> Context<R> {
    fn frame(&self) -> &Frame<R> {
        // safety: see above
        unsafe { self.frame.as_ref() }
    }

    fn frame_mut(&mut self) -> &mut Frame<R> {
        // safety: see above
        unsafe { self.frame.as_mut() }
    }

    fn platform_mut(&mut self) -> &mut R {
        // safety: see above
        unsafe { self.platform.as_mut() }
    }
}
