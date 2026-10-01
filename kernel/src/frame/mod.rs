pub mod animation;
pub mod interaction;
pub mod layout;
pub mod paint;
pub mod timer;
pub mod transition;

use std::{
    any::TypeId,
    collections::HashMap,
    hash::{BuildHasherDefault, Hasher},
    marker::PhantomData,
    time::Duration,
};

use crate::{
    Atom, Clip, Content, FrameInfo, Widget,
    animation::{Easing, Transition, TransitionProperties},
    arena::{DataArena, DataId},
    geometry::{Constraints, LogicalPoint, LogicalRect, LogicalSize, Sides},
    input::Input,
    interact::{Interaction, Sense, WidgetId},
    layout::Layout,
};

/// typestate modes for [`crate::Ui`]
///
/// every mode can insert content and access shared frame services
pub mod state {
    use super::PhantomData;

    /// an unlaid node that may establish a layout
    pub struct Build;

    /// an unlaid child with layout item `I`
    pub struct Child<I>(PhantomData<I>);

    /// a laid-out node that may create children
    pub struct Open<L>(PhantomData<L>);

    /// access to an existing node without layout or child creation
    pub struct Node;
}

/// scoped handle for building a frame node
///
/// its [`state`] mode determines which operations are available
pub struct Ui<'ui, C, S = state::Build> {
    inner: UiInner<'ui, C>,
    marker: PhantomData<S>,
}

impl<'ui, C, S> Ui<'ui, C, S> {
    /// identifies this node for references within the current render
    pub fn id(&self) -> NodeId {
        self.inner.node
    }

    pub fn clip<X: Clip<C>>(self, clip: X) -> Self {
        let node = self.inner.node;
        let clip = self.inner.frame.store_clip(clip);
        self.inner.frame.inner.nodes[node.index()].clip = clip;
        self
    }

    /// returns this node's widget id
    ///
    /// without an explicit id, it derives from the parent's id and child position
    pub fn current_widget_id(&self) -> WidgetId {
        self.inner.frame.inner.nodes[self.inner.node.index()].widget_id
    }

    pub fn hit(self, hit: Sides) -> Self {
        let node = self.inner.node;
        self.inner.frame.inner.geometry_mut(node).hit = hit;
        self
    }

    pub fn transition(self, transition: Transition) -> Self {
        let node = self.inner.node;
        self.inner.frame.inner.geometry_mut(node).transition = Some(transition);
        self
    }

    /// selects the parent for stacking, clipping and containing size
    pub fn parent(mut self, parent: impl Into<NodeTarget>) -> Self {
        let node = self.inner.node;
        let frame = self.inner.frame_mut();
        let parent = frame.resolve_target(node, parent.into());
        frame.nodes[node.index()].parent = parent;
        self
    }

    /// sets this node's paint order among its visual siblings
    pub fn z_index(mut self, z_index: i16) -> Self {
        let node = self.inner.node;
        let frame = self.inner.frame_mut();
        frame.nodes[node.index()].z_index = z_index;
        self
    }

    /// inserts content into the current node
    ///
    /// atoms contribute to sizing only when the node has no layout.
    pub fn insert<X: Content<C>>(&mut self, content: X) -> X::Response {
        content.append(Ui {
            inner: UiInner {
                frame: &mut *self.inner.frame,
                context: &mut *self.inner.context,
                node: self.inner.node,
                next_child: 0,
                owns_node: false,
            },
            marker: PhantomData,
        })
    }
}

impl<'ui, C> Ui<'ui, C, state::Build> {
    /// assigns an absolute widget id before this node is given children
    pub fn widget_id(mut self, id: WidgetId) -> Self {
        self.inner.set_widget_id(id);
        self
    }

    /// builds a widget in this node
    pub fn build<W: Widget<C>>(self, widget: W) -> W::Response {
        widget.build(self)
    }

    /// establishes the current node's layout
    pub fn layout<L: Layout>(self, layout: L) -> Ui<'ui, C, state::Open<L>> {
        let Ui { inner, .. } = self;
        let node = inner.node;
        let layout = inner.frame.inner.store_layout(layout);
        inner.frame.inner.nodes[node.index()].layout = layout;
        Ui {
            inner,
            marker: PhantomData,
        }
    }
}

impl<'ui, C, I: 'static> Ui<'ui, C, state::Child<I>> {
    /// assigns an absolute widget id before this node is given children
    pub fn widget_id(mut self, id: WidgetId) -> Self {
        self.inner.set_widget_id(id);
        self
    }

    /// sets this child's layout item
    #[inline]
    pub fn item(mut self, item: I) -> Self {
        let node = self.inner.node;
        let frame = self.inner.frame_mut();
        let id = frame.nodes[node.index()].item;
        if id.offset().is_some() {
            *frame.data.load_mut(id) = item;
        } else {
            frame.nodes[node.index()].item = frame.data.store(item);
        }
        self
    }

    /// builds a widget in this node
    pub fn build<W: Widget<C>>(self, widget: W) -> W::Response {
        widget.build(Ui {
            inner: self.inner,
            marker: PhantomData,
        })
    }

    /// establishes the current node's layout
    pub fn layout<L: Layout>(self, layout: L) -> Ui<'ui, C, state::Open<L>> {
        let ui: Ui<'_, C> = Ui {
            inner: self.inner,
            marker: PhantomData,
        };
        ui.layout(layout)
    }
}

impl<'ui, C, L: Layout> Ui<'ui, C, state::Open<L>> {
    /// assigns an absolute widget id before this node is given children
    pub fn widget_id(mut self, id: WidgetId) -> Self {
        self.inner.set_widget_id(id);
        self
    }

    /// creates a child with this layout's shared default item
    #[inline]
    pub fn child(&mut self) -> Ui<'_, C, state::Child<L::Item>> {
        let node = self.inner.push_child();
        Ui::new(&mut *self.inner.context, &mut *self.inner.frame, node)
    }

    pub fn offset(mut self, offset: LogicalPoint) -> Self {
        let node = self.inner.node;
        let frame = self.inner.frame_mut();
        let layout = frame.nodes[node.index()].layout.index().unwrap();
        frame.layouts[layout].offset = offset;
        self
    }

    /// creates an out-of-flow child positioned relative to `relative`
    pub fn relative(&mut self, relative: impl Into<NodeTarget>) -> Ui<'_, C> {
        let node = self.inner.push_child();
        let frame = self.inner.frame_mut();
        frame.nodes[node.index()].relative = frame.resolve_target(node, relative.into());
        frame.nodes[node.index()].out_of_flow = true;
        Ui::new(&mut *self.inner.context, &mut *self.inner.frame, node)
    }
}

impl<C, S> Ui<'_, C, S> {
    /// returns previous frame geometry and tracks this id for the next layout
    pub fn geometry(&mut self, id: WidgetId) -> Option<LogicalRect> {
        self.inner.frame.inner.requests.entry(id).or_insert(Request::Geometry);
        self.inner
            .frame
            .inner
            .geometry_previous
            .iter()
            .find_map(|(candidate, area)| (*candidate == id).then_some(*area))
    }

    /// requests interaction for this node using its widget id
    pub fn interact(&mut self, sense: Sense) -> Interaction {
        let id = self.current_widget_id();
        self.interact_widget(id, sense)
    }

    /// requests interaction for a node assigned this exact widget id
    ///
    /// the node may be built later
    pub fn interact_widget(&mut self, id: WidgetId, sense: Sense) -> Interaction {
        let frame = self.inner.frame_mut();
        frame
            .requests
            .entry(id)
            .and_modify(|request| match request {
                Request::Geometry => *request = Request::Interaction(sense),
                Request::Interaction(previous) => {
                    previous.click |= sense.click;
                    previous.drag |= sense.drag;
                    previous.focus |= sense.focus;
                    previous.scroll |= sense.scroll;
                }
            })
            .or_insert(Request::Interaction(sense));
        let interaction = frame.interaction.response(id);
        if interaction.activated || interaction.deactivated || interaction.clicked {
            frame.frame_requested = true;
        }
        interaction
    }

    pub fn input(&self) -> &Input {
        &self.inner.frame.inner.input
    }

    /// accesses context resources during frame construction
    ///
    /// drawing remains deferred to [`Atom`] implementations
    pub fn context(&mut self) -> &mut C {
        self.inner.context
    }

    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.inner.frame.inner.interaction.is_focused(id)
    }

    pub fn focus(&mut self, id: WidgetId) {
        let frame = self.inner.frame_mut();
        if frame.interaction.focus(id) {
            frame.frame_requested = true;
        }
    }

    pub fn clear_focus(&mut self) {
        let frame = self.inner.frame_mut();
        if frame.interaction.clear_focus() {
            frame.frame_requested = true;
        }
    }

    pub fn pointer_position(&self) -> Option<LogicalPoint> {
        self.inner.frame.inner.interaction.pointer_position()
    }

    pub fn screen(&self) -> LogicalRect {
        self.inner.frame.inner.screen
    }

    pub fn time(&self) -> Duration {
        self.inner.frame.inner.time
    }

    pub fn animate(&mut self, id: WidgetId, target: f32, duration: Duration, easing: Easing) -> f32 {
        let frame = self.inner.frame_mut();
        let time = frame.time;
        animation::AnimationState::update(&mut frame.animations, id, target, |animation| {
            animation.advance(target, duration, easing, time)
        })
    }

    pub fn animate_loop(&mut self, id: WidgetId, duration: Duration, easing: Easing) -> f32 {
        let frame = self.inner.frame_mut();
        let time = frame.time;
        animation::AnimationState::update(&mut frame.animations, id, 0.0, |animation| {
            animation.advance_loop(duration, easing, time)
        })
    }

    pub fn timer(&mut self, id: WidgetId, duration: Duration) -> bool {
        let frame = self.inner.frame_mut();
        timer::TimerState::update(&mut frame.timers, id, duration, None, frame.time)
    }

    pub fn timer_loop(&mut self, id: WidgetId, duration: Duration) -> bool {
        assert!(!duration.is_zero(), "looping timer duration must be nonzero");
        let frame = self.inner.frame_mut();
        timer::TimerState::update(&mut frame.timers, id, duration, Some(duration), frame.time)
    }

    pub fn request_frame(&mut self) {
        self.inner.frame.inner.frame_requested = true;
    }
}

// all atoms are content
impl<C, A: Atom<C>> Content<C> for A {
    type Response = ();

    fn append(self, ui: Ui<'_, C, state::Node>) {
        let node = ui.inner.node;
        ui.inner.frame.push_atom(node, self);
    }
}

/// identifies a node only during the current render
///
/// do not store this across renders
#[cfg_attr(not(debug_assertions), repr(transparent))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId {
    value: u32,
    #[cfg(debug_assertions)]
    generation: u16,
}

/// selects a node for parenting or relative positioning
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NodeTarget {
    /// the node's current positioning reference
    #[default]
    Parent,
    /// an earlier node in the current render
    Node(NodeId),
    /// an earlier node whose unique widget id is already assigned
    Widget(WidgetId),
    /// the frame root
    Root,
}

impl From<WidgetId> for NodeTarget {
    fn from(id: WidgetId) -> Self {
        Self::Widget(id)
    }
}

impl From<NodeId> for NodeTarget {
    fn from(id: NodeId) -> Self {
        Self::Node(id)
    }
}

//
// internals
//

include!("graph.rs");

struct UiInner<'ui, C> {
    frame: &'ui mut Frame<C>,
    context: &'ui mut C,
    node: NodeId,
    next_child: u32,
    owns_node: bool,
}

impl<C> UiInner<'_, C> {
    fn set_widget_id(&mut self, id: WidgetId) {
        let node = self.node;
        assert_eq!(
            self.next_child, 0,
            "a widget id must be assigned before children are built"
        );
        self.frame.inner.nodes[node.index()].widget_id = id;
    }

    fn push_child(&mut self) -> NodeId {
        let slot = self.next_child;
        self.next_child = slot.checked_add(1).expect("child id overflow");
        let id = self.frame.inner.nodes[self.node.index()].widget_id.child(slot);
        self.frame.inner.push_node(Some(self.node), id)
    }

    #[inline]
    fn frame_mut(&mut self) -> &mut FrameInner {
        &mut self.frame.inner
    }
}

impl<'ui, C, S> Ui<'ui, C, S> {
    fn new(context: &'ui mut C, frame: &'ui mut Frame<C>, node: NodeId) -> Self {
        Self {
            inner: UiInner {
                frame,
                context,
                node,
                next_child: 0,
                owns_node: true,
            },
            marker: PhantomData,
        }
    }
}

impl<C> Drop for UiInner<'_, C> {
    fn drop(&mut self) {
        if !self.owns_node {
            return;
        }
        let node = self.node;
        let frame = self.frame_mut();
        frame.nodes[node.index()].subtree_end = u32::try_from(frame.nodes.len() - 1).expect("too many frame nodes");
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
