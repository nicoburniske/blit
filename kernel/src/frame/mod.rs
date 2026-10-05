use crate::{Context, Scalar};
pub mod animation;
pub mod interaction;
pub mod layout;
pub mod paint;
pub mod position;
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
    animation::{Easing, Transition},
    arena::{DataArena, DataId, Scratch},
    geometry::{Constraints, Point, Rect, Sides, Size},
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
pub struct Ui<'ui, C: Context, S = state::Build> {
    inner: UiInner<'ui, C>,
    marker: PhantomData<S>,
}

impl<'ui, C: Context, S> Ui<'ui, C, S> {
    /// identifies this node for references within the current render
    pub fn id(&self) -> NodeId {
        self.inner.node
    }

    pub fn clip<X: Clip<C>>(self, clip: X) -> Self {
        let clip = self.inner.frame.store_clip(clip);
        self.inner.frame.nodes[self.inner.node.index()].clip = clip;
        self
    }

    /// returns this node's widget id
    ///
    /// without an explicit id, it derives from the parent's id and child position
    pub fn current_widget_id(&self) -> WidgetId {
        self.inner.frame.nodes[self.inner.node.index()].widget_id
    }

    pub fn hit(self, hit: Sides<C::Scalar>) -> Self {
        self.inner.frame.geometry_mut(self.inner.node).hit = hit;
        self
    }

    pub fn transition(self, transition: Transition) -> Self {
        self.inner.frame.geometry_mut(self.inner.node).transition = Some(transition);
        self
    }

    /// selects the parent for stacking, clipping and containing size
    ///
    /// named targets must already exist. positioning stays with its anchor.
    #[inline]
    pub fn parent(self, target: impl Into<NodeTarget>) -> Self {
        let parent = self.inner.frame.resolve_target(self.inner.node, target.into());
        self.inner.frame.nodes[self.inner.node.index()].visual_parent = parent;
        self
    }

    /// positions this node against a reference outside its parent's flow
    pub fn relative(self, target: impl Into<NodeTarget>) -> Self {
        let reference = self.inner.frame.resolve_target(self.inner.node, target.into());
        let node = &mut self.inner.frame.nodes[self.inner.node.index()];
        node.relative = reference;
        node.out_of_flow = true;
        self
    }

    /// sets this node's paint order among its visual siblings
    pub fn z_index(self, z_index: i16) -> Self {
        self.inner.frame.node_geometry[self.inner.node.index()].z_index = z_index;
        self
    }

    /// inserts content into the current node
    ///
    /// layouts choose whether their atoms contribute to sizing
    pub fn insert<X: Content<C>>(&mut self, content: X) -> X::Response {
        content.append(Ui {
            inner: UiInner {
                frame: self.inner.frame,
                context: self.inner.context,
                scratch: self.inner.scratch,
                node: self.inner.node,
                next_child: 0,
                owns_node: false,
            },
            marker: PhantomData,
        })
    }

    /// temporary storage that stays usable while building this node
    pub fn scratch<T: Copy>(&self, len: usize, value: T) -> Scratch<'ui, T> {
        self.inner.scratch.scratch(len, value)
    }
}

impl<'ui, C: Context> Ui<'ui, C, state::Build> {
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
    pub fn layout<L: Layout<C>>(self, layout: L) -> Ui<'ui, C, state::Open<L>> {
        let Ui { inner, .. } = layout.on_insert(self);
        inner.frame.store_layout(inner.node, layout);
        Ui {
            inner,
            marker: PhantomData,
        }
    }
}

impl<'ui, C: Context, I: 'static> Ui<'ui, C, state::Child<I>> {
    /// assigns an absolute widget id before this node is given children
    pub fn widget_id(mut self, id: WidgetId) -> Self {
        self.inner.set_widget_id(id);
        self
    }

    /// sets this child's layout item
    #[inline]
    pub fn item(self, item: I) -> Self {
        let node = &mut self.inner.frame.nodes[self.inner.node.index()];
        if node.item.offset().is_some() {
            *self.inner.frame.data.load_mut(node.item) = item;
        } else {
            node.item = self.inner.frame.data.store(item);
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
    pub fn layout<L: Layout<C>>(self, layout: L) -> Ui<'ui, C, state::Open<L>> {
        let ui: Ui<'_, C> = Ui {
            inner: self.inner,
            marker: PhantomData,
        };
        ui.layout(layout)
    }
}

impl<'ui, C: Context, L: Layout<C>> Ui<'ui, C, state::Open<L>> {
    /// assigns an absolute widget id before this node is given children
    pub fn widget_id(mut self, id: WidgetId) -> Self {
        self.inner.set_widget_id(id);
        self
    }

    /// creates a child with this layout's shared default item
    #[inline]
    pub fn child(&mut self) -> Ui<'_, C, state::Child<L::Item>> {
        let node = self.inner.push_child();
        Ui::new(self.inner.frame, self.inner.context, self.inner.scratch, node)
    }
}

impl<C: Context, S> Ui<'_, C, S> {
    /// returns previous frame geometry and tracks this id for the next layout
    pub fn geometry(&mut self, id: WidgetId) -> Option<Rect<C::Scalar>> {
        self.inner.frame.requests.entry(id).or_insert(Request::Geometry);
        self.inner
            .frame
            .geometry_previous
            .iter()
            .find_map(|(candidate, area)| (*candidate == id).then_some(*area))
    }

    /// requests interaction for this node using its widget id
    pub fn interact(&mut self, sense: Sense) -> Interaction<C::Scalar> {
        self.interact_widget(self.current_widget_id(), sense)
    }

    /// requests interaction for a node assigned this exact widget id
    ///
    /// the node may be built later
    pub fn interact_widget(&mut self, id: WidgetId, sense: Sense) -> Interaction<C::Scalar> {
        let frame = &mut *self.inner.frame;
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
            frame.request_frame();
        }
        interaction
    }

    pub fn input(&self) -> &Input<C::Scalar> {
        &self.inner.frame.input
    }

    /// accesses context resources during frame construction
    ///
    /// drawing remains deferred to [`Atom`] implementations
    pub fn context(&mut self) -> &mut C {
        self.inner.context
    }

    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.inner.frame.interaction.is_focused(id)
    }

    pub fn focus(&mut self, id: WidgetId) {
        if self.inner.frame.interaction.focus(id) {
            self.inner.frame.request_frame();
        }
    }

    pub fn clear_focus(&mut self) {
        if self.inner.frame.interaction.clear_focus() {
            self.inner.frame.request_frame();
        }
    }

    pub fn pointer_position(&self) -> Option<Point<C::Scalar>> {
        self.inner.frame.interaction.pointer_position()
    }

    pub fn screen(&self) -> Rect<C::Scalar> {
        self.inner.frame.screen
    }

    pub fn time(&self) -> Duration {
        self.inner.frame.time
    }

    pub fn animate(&mut self, id: WidgetId, target: f32, duration: Duration, easing: Easing) -> f32 {
        let frame = &mut *self.inner.frame;
        animation::AnimationState::update(&mut frame.animations, id, target, |animation| {
            animation.advance(target, duration, easing, frame.time)
        })
    }

    pub fn animate_loop(&mut self, id: WidgetId, duration: Duration, easing: Easing) -> f32 {
        let frame = &mut *self.inner.frame;
        animation::AnimationState::update(&mut frame.animations, id, 0.0, |animation| {
            animation.advance_loop(duration, easing, frame.time)
        })
    }

    pub fn timer(&mut self, id: WidgetId, duration: Duration) -> bool {
        let frame = &mut *self.inner.frame;
        timer::TimerState::update(&mut frame.timers, id, duration, None, frame.time)
    }

    pub fn timer_loop(&mut self, id: WidgetId, duration: Duration) -> bool {
        assert!(!duration.is_zero(), "looping timer duration must be nonzero");
        let frame = &mut *self.inner.frame;
        timer::TimerState::update(&mut frame.timers, id, duration, Some(duration), frame.time)
    }

    pub fn request_frame(&mut self) {
        self.inner.frame.request_frame();
    }
}

// all atoms are content
impl<C: Context, A: Atom<C>> Content<C> for A {
    type Response = ();

    fn append(self, ui: Ui<'_, C, state::Node>) {
        ui.inner.frame.push_atom(ui.inner.node, self);
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

/// selects a node for visual parenting or absolute positioning
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NodeTarget {
    /// the node's structural parent
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

struct UiInner<'ui, C: Context> {
    frame: &'ui mut Frame<C>,
    context: &'ui mut C,
    scratch: &'ui DataArena,
    node: NodeId,
    next_child: u32,
    owns_node: bool,
}

impl<C: Context> UiInner<'_, C> {
    fn set_widget_id(&mut self, id: WidgetId) {
        assert_eq!(
            self.next_child, 0,
            "a widget id must be assigned before children are built"
        );
        self.frame.nodes[self.node.index()].widget_id = id;
    }

    #[inline]
    fn push_child(&mut self) -> NodeId {
        let slot = self.next_child;
        self.next_child = slot.checked_add(1).expect("child id overflow");
        let id = self.frame.nodes[self.node.index()].widget_id.child(slot);
        self.frame.push_node(Some(self.node), id)
    }
}

impl<'ui, C: Context, S> Ui<'ui, C, S> {
    fn new(frame: &'ui mut Frame<C>, context: &'ui mut C, scratch: &'ui DataArena, node: NodeId) -> Self {
        Self {
            inner: UiInner {
                frame,
                context,
                scratch,
                node,
                next_child: 0,
                owns_node: true,
            },
            marker: PhantomData,
        }
    }
}

impl<C: Context> Drop for UiInner<'_, C> {
    fn drop(&mut self) {
        if !self.owns_node {
            return;
        }
        self.frame.nodes[self.node.index()].subtree_end =
            u32::try_from(self.frame.nodes.len() - 1).expect("too many frame nodes");
    }
}

impl NodeId {
    #[inline]
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
