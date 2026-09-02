pub mod animation;
pub mod interaction;
pub mod layout;
pub mod paint;
pub mod position;
pub mod timer;
pub mod transition;

use std::{
    any::TypeId,
    marker::PhantomData,
    mem::{ManuallyDrop, size_of},
    num::NonZeroU16,
    ptr::{self, NonNull},
    time::Duration,
};

use crate::{
    Atom, Clip, FrameInfo, Platform, Widget,
    animation::{Easing, Transition},
    arena::{DataArena, DataId},
    geometry::{Constraints, Point, Rect, Sides, Size},
    input::Input,
    interact::{Interaction, Sense, WidgetId},
    layout::{Axis, Layout, LayoutResolution},
};

/// ui build states
pub mod state {
    use super::{PhantomData, Place};

    /// state of a ui passed to [`Widget::build`](crate::Widget::build)
    pub struct Build;

    /// state of a ui with an active layout
    pub struct Open<L> {
        pub(super) marker: PhantomData<L>,
    }

    /// state of a child waiting to be populated
    ///
    /// `P` is the parent layout and `I` tracks its required item
    pub struct Pending<P, I = ()> {
        pub(super) place: Place,
        pub(super) item: I,
        pub(super) marker: PhantomData<P>,
    }
}

/// scoped handle for building a frame node
///
/// [`state::Build`]: unpopulated node passed to a widget
/// [`state::Open`]: laid-out node that accepts children
/// [`state::Pending`]: new child awaiting a widget or layout
pub struct Ui<'ui, R: Platform, S = state::Build> {
    context: &'ui mut Context<R>,
    node: NodeId,
    state: S,
    open: bool,
}

impl<'ui, R: Platform, S> Ui<'ui, R, S> {
    pub fn id(&self) -> NodeId {
        self.node
    }

    pub fn clip<C: Clip<R>>(self, clip: C) -> Self {
        let node = self.node;
        let frame = self.context.frame_mut();
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
        self.context.frame_mut().set_absolute(node, absolute);
        self
    }

    pub fn widget_id(self, id: WidgetId) -> Self {
        let node = self.node;
        self.context.frame_mut().set_id(node, id);
        self
    }

    pub fn hit(self, hit: Sides) -> Self {
        let node = self.node;
        self.context.frame_mut().set_hit(node, hit);
        self
    }

    pub fn transition(self, transition: Transition) -> Self {
        let node = self.node;
        self.context.frame_mut().set_transition(node, transition);
        self
    }
}

impl<'ui, R: Platform> Ui<'ui, R, state::Build> {
    /// inserts an atom into the current node
    pub fn insert<A: Atom<R>>(&mut self, atom: A) -> &mut Self {
        let node = self.node;
        self.context.frame_mut().push_atom(node, atom);
        self
    }

    /// establishes the current node's layout
    pub fn layout<L: Layout<R>>(self, layout: L) -> Ui<'ui, R, state::Open<L>> {
        let (context, node, _) = self.into_parts();
        let frame = context.frame_mut();
        assert!(
            frame.nodes[node.index()].layout.index().is_none(),
            "node already has a layout"
        );
        let layout = frame.store_layout(layout);
        frame.nodes[node.index()].layout = layout;
        open_ui(context, node)
    }

    pub fn new_layer(&mut self) -> LayerId {
        self.context.frame_mut().add_layer()
    }
}

impl<'ui, R: Platform, L: Layout<R>> Ui<'ui, R, state::Open<L>> {
    pub fn offset(self, offset: Point) -> Self {
        let node = self.node;
        let frame = self.context.frame_mut();
        let layout = frame.nodes[node.index()].layout.index().unwrap();
        frame.layouts[layout].offset = offset;
        self
    }

    /// inserts a widget into the current node
    pub fn insert<W: Widget<R>>(&mut self, widget: W) -> W::Response {
        let node = self.node;
        widget.build(Ui::new(&mut *self.context, node, state::Build, false))
    }

    pub fn new_layer(&mut self) -> LayerId {
        self.context.frame_mut().add_layer()
    }

    /// creates a pending child
    #[must_use = "a child must be populated with insert or layout"]
    pub fn child(&mut self) -> Ui<'_, R, state::Pending<L>> {
        let node = self.context.frame_mut().push_node(None);
        Ui::new(
            &mut *self.context,
            node,
            state::Pending {
                place: Place::new(),
                item: (),
                marker: PhantomData,
            },
            false,
        )
    }
}

impl<'ui, R: Platform, L: Layout<R, Item = ()>> Ui<'ui, R, state::Open<L>> {
    /// adds a child widget with default placement
    pub fn add<W: Widget<R>>(&mut self, widget: W) -> W::Response {
        self.child().insert(widget)
    }
}

impl<'ui, R: Platform, P: Layout<R>, I> Ui<'ui, R, state::Pending<P, I>> {
    pub fn item(self, item: P::Item) -> Ui<'ui, R, state::Pending<P, P::Item>> {
        let (context, node, state) = self.into_parts();
        Ui::new(
            context,
            node,
            state::Pending {
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

impl<'ui, R: Platform, P: Layout<R>> Ui<'ui, R, state::Pending<P, P::Item>> {
    /// inserts a widget into the pending child
    pub fn insert<W: Widget<R>>(self, widget: W) -> W::Response {
        let (context, node, state) = self.into_parts();
        let response = context.build_node(node, widget);
        let frame = context.frame_mut();
        frame.set_place(node, state.place);
        let item = frame.data.store(state.item);
        frame.nodes[node.index()].item = item;
        response
    }

    /// establishes the pending child's layout
    pub fn layout<L: Layout<R>>(self, layout: L) -> Ui<'ui, R, state::Open<L>> {
        let (context, node, state) = self.into_parts();
        let frame = context.frame_mut();
        let layout = frame.store_layout(layout);
        frame.nodes[node.index()].layout = layout;
        frame.set_place(node, state.place);
        let item = frame.data.store(state.item);
        frame.nodes[node.index()].item = item;
        open_ui(context, node)
    }
}

impl<R: Platform, S> Ui<'_, R, S> {
    pub fn set_id(&mut self, node: NodeId, id: WidgetId) {
        self.context.frame_mut().set_id(node, id);
    }

    pub fn set_hit(&mut self, node: NodeId, hit: Sides) {
        self.context.frame_mut().set_hit(node, hit);
    }

    pub fn set_transition(&mut self, node: NodeId, transition: Transition) {
        self.context.frame_mut().set_transition(node, transition);
    }

    pub fn geometry(&self, id: WidgetId) -> Option<Rect> {
        self.context.frame().geometry(id)
    }

    pub fn interact(&mut self, id: WidgetId, sense: Sense) -> Interaction {
        let frame = self.context.frame_mut();
        let interaction = frame.interaction.response(id, sense);
        if interaction.activated || interaction.deactivated || interaction.clicked {
            frame.request_frame();
        }
        interaction
    }

    pub fn input(&self) -> &Input {
        &self.context.frame().input
    }

    /// accesses platform resources during frame construction
    ///
    /// drawing remains deferred to [`Atom`] implementations
    pub fn platform(&mut self) -> &mut R {
        self.context.platform_mut()
    }

    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.context.frame().interaction.is_focused(id)
    }

    pub fn focus(&mut self, id: WidgetId) {
        let frame = self.context.frame_mut();
        if frame.interaction.focus(id) {
            frame.request_frame();
        }
    }

    pub fn clear_focus(&mut self) {
        let frame = self.context.frame_mut();
        if frame.interaction.clear_focus() {
            frame.request_frame();
        }
    }

    pub fn pointer_position(&self) -> Option<Point> {
        self.context.frame().interaction.pointer_position()
    }

    pub fn screen(&self) -> Rect {
        self.context.frame().screen
    }

    pub fn time(&self) -> Duration {
        self.context.frame().time
    }

    pub fn animate(
        &mut self,
        id: WidgetId,
        target: f32,
        duration: Duration,
        easing: Easing,
    ) -> f32 {
        let frame = self.context.frame_mut();
        let time = frame.time;
        animation::AnimationState::update(&mut frame.animations, id, target, |animation| {
            animation.advance(target, duration, easing, time)
        })
    }

    pub fn animate_loop(&mut self, id: WidgetId, duration: Duration, easing: Easing) -> f32 {
        let frame = self.context.frame_mut();
        let time = frame.time;
        animation::AnimationState::update(&mut frame.animations, id, 0.0, |animation| {
            animation.advance_loop(duration, easing, time)
        })
    }

    pub fn timer(&mut self, id: WidgetId, duration: Duration) -> bool {
        let frame = self.context.frame_mut();
        timer::TimerState::update(&mut frame.timers, id, duration, None, frame.time)
    }

    pub fn timer_loop(&mut self, id: WidgetId, duration: Duration) -> bool {
        assert!(
            !duration.is_zero(),
            "looping timer duration must be nonzero"
        );
        let frame = self.context.frame_mut();
        timer::TimerState::update(&mut frame.timers, id, duration, Some(duration), frame.time)
    }

    pub fn request_frame(&mut self) {
        self.context.frame_mut().request_frame();
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

//
// internals
//

include!("graph.rs");

impl<'ui, R: Platform, S> Ui<'ui, R, S> {
    fn new(context: &'ui mut Context<R>, node: NodeId, state: S, open: bool) -> Self {
        Self {
            context,
            node,
            state,
            open,
        }
    }

    fn into_parts(self) -> (&'ui mut Context<R>, NodeId, S) {
        let node = self.node;
        let this = ManuallyDrop::new(self);
        // safety: this is not dropped and each field is read once
        unsafe { (ptr::read(&this.context), node, ptr::read(&this.state)) }
    }
}

impl<R: Platform, S> Drop for Ui<'_, R, S> {
    fn drop(&mut self) {
        if !self.open {
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

fn open_ui<R: Platform, L: Layout<R>>(
    context: &mut Context<R>,
    node: NodeId,
) -> Ui<'_, R, state::Open<L>> {
    context.frame_mut().current_parent = Some(node);
    Ui::new(
        context,
        node,
        state::Open {
            marker: PhantomData,
        },
        true,
    )
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
    fn build_node<W: Widget<R>>(&mut self, node: NodeId, widget: W) -> W::Response {
        let parent = self.frame().current_parent;
        self.frame_mut().current_parent = Some(node);
        let output = widget.build(Ui::new(self, node, state::Build, false));
        let frame = self.frame_mut();
        frame.nodes[node.index()].subtree_end =
            u32::try_from(frame.nodes.len() - 1).expect("too many frame nodes");
        frame.current_parent = parent;
        assert!(
            frame.nodes[node.index()].first_atom.index().is_some()
                || frame.nodes[node.index()].layout.index().is_some(),
            "widget did not populate its node"
        );
        output
    }

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
