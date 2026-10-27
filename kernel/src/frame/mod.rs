use crate::{Context, Scalar};
pub mod animation;
pub mod atom;
pub mod bump;
pub mod interaction;
pub mod layout;
pub mod paint;
pub mod position;
pub mod timer;
pub mod transition;

use std::{any::TypeId, marker::PhantomData, time::Duration};

use self::bump::{BumpArena, BumpId, FrameSlice, Scratch};
use crate::{
    Atom, AtomWidget, Clip, ClipShape, FrameInfo, IntoWidget, Widget,
    animation::{Easing, Transition},
    geometry::{Constraints, Point, Rect, Sides, Size},
    input::Input,
    interact::{Id, Interact},
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
    pub fn node_id(&self) -> NodeId {
        self.inner.node
    }

    /// stores values that atoms can read until the next build
    pub fn store_slice<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> FrameSlice<T> {
        self.inner.frame.bump.store_slice(values)
    }

    pub fn clip<X: Clip<C>>(self, clip: X) -> Self {
        let clip = self.inner.frame.store_clip(clip);
        self.inner.frame.nodes[self.inner.node.index()].clip = clip;
        self
    }

    /// returns this node's stable id
    ///
    /// without an explicit id, it derives from the parent's id and child position
    pub fn current_id(&self) -> Id {
        self.inner.frame.nodes[self.inner.node.index()].widget_id
    }

    pub fn hit(self, hit: Sides<C::Scalar>) -> Self {
        self.inner.frame.bounds_mut(self.inner.node).hit = hit;
        self
    }

    pub fn transition(self, transition: Transition) -> Self {
        self.inner.frame.bounds_mut(self.inner.node).transition = Some(transition);
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
        self.inner.frame.placements[self.inner.node.index()].z_index = z_index;
        self
    }

    /// inserts content into the current node
    ///
    /// layouts choose whether their atoms contribute to sizing
    pub fn insert<T, W>(&mut self, widget: T) -> W::Response
    where
        T: IntoWidget<C, W>,
        W: Widget<C, state::Node>,
    {
        widget.into_widget().build(Ui {
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
    /// assigns an absolute id before this node is given children
    pub fn id(mut self, id: Id) -> Self {
        self.inner.set_widget_id(id);
        self
    }

    /// builds a widget in this node
    pub fn build<W: Widget<C>>(self, widget: W) -> W::Response {
        widget.build(self)
    }

    /// builds a flow subtree and measures its current animated size under `bounds`
    pub fn build_measured<W: Widget<C>>(
        self,
        bounds: Constraints<C::Scalar>,
        widget: W,
    ) -> (W::Response, Size<C::Scalar>) {
        let mut inner = self.inner;
        inner.owns_node = false;
        let node = inner.node;
        let response = widget.build(Ui::new(inner.frame, inner.context, inner.scratch, node));
        assert!(
            !inner.frame.nodes[node.index()].out_of_flow,
            "measured widgets must stay in flow"
        );
        let mut bump = std::mem::take(&mut inner.frame.bump);
        bump.prepare_scratch();
        let size = transition::resolve(inner.frame, &bump, inner.context, node, bounds, false);
        inner.frame.bump = bump;
        (response, size)
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
    /// assigns an absolute id before this node is given children
    pub fn id(mut self, id: Id) -> Self {
        self.inner.set_widget_id(id);
        self
    }

    /// sets this child's layout item
    #[inline]
    pub fn item(self, item: I) -> Self {
        let node = &mut self.inner.frame.nodes[self.inner.node.index()];
        if node.item.offset().is_some() {
            *self.inner.frame.bump.load_mut(node.item) = item;
        } else {
            node.item = self.inner.frame.bump.store(item);
        }
        self
    }

    /// builds a widget in this node
    pub fn build<W: Widget<C>>(self, widget: W) -> W::Response {
        widget.build(self.into())
    }

    pub fn build_measured<W: Widget<C>>(
        self,
        bounds: Constraints<C::Scalar>,
        widget: W,
    ) -> (W::Response, Size<C::Scalar>) {
        Ui::<C>::from(self).build_measured(bounds, widget)
    }

    /// establishes the current node's layout
    pub fn layout<L: Layout<C>>(self, layout: L) -> Ui<'ui, C, state::Open<L>> {
        Ui::<C>::from(self).layout(layout)
    }
}

impl<'ui, C: Context, L: Layout<C>> Ui<'ui, C, state::Open<L>> {
    /// assigns an absolute id before this node is given children
    pub fn id(mut self, id: Id) -> Self {
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
    /// returns the area from the previous layout and tracks this id for the next one
    pub fn area(&mut self, id: Id) -> Option<Rect<C::Scalar>> {
        self.inner.frame.areas_requested.push(id);
        let areas = &self.inner.frame.areas_previous;
        areas
            .binary_search_by_key(&id, |(candidate, _)| *candidate)
            .ok()
            .map(|index| areas[index].1)
    }

    /// retains `kind` for this node while builds keep requesting it
    ///
    /// `kind` is used once. returns its response from the update before this build
    /// or its first update without resolved geometry or routed input when newly registered
    /// request each interaction kind only once per widget in a build
    pub fn interact<K: Interact<C::Scalar>>(&mut self, kind: K) -> K::Response {
        let id = self.current_id();
        let node = self.inner.node;
        let frame = &mut *self.inner.frame;
        frame.nodes[node.index()].interactive = true;
        frame.interaction.interact(id, Some(node.value), kind, frame.time)
    }

    /// interacts as a node assigned this exact widget id, which may be built later
    /// request each interaction kind only once per widget in a build
    pub fn interact_widget<K: Interact<C::Scalar>>(&mut self, id: Id, kind: K) -> K::Response {
        let frame = &mut *self.inner.frame;
        frame.interaction.interact(id, None, kind, frame.time)
    }

    /// access context during frame construction
    pub fn context(&self) -> &C {
        self.inner.context
    }

    /// access mutable context during frame construction
    pub fn context_mut(&mut self) -> &mut C {
        self.inner.context
    }

    /// returns the focused widget in this node's built subtree including this node
    ///
    /// call after building children to include them. follows structural parenting.
    pub fn focused(&self) -> Option<Id> {
        let focused = self.inner.frame.interaction.focused?;
        self.contains_widget(focused).then_some(focused)
    }

    /// checks this node and its children already built in the current frame
    pub fn contains_widget(&self, id: Id) -> bool {
        self.inner.frame.nodes[self.inner.node.index()..]
            .iter()
            .any(|node| node.widget_id == id)
    }

    pub fn is_focused(&self, id: Id) -> bool {
        self.inner.frame.interaction.focused == Some(id)
    }

    pub fn focus(&mut self, id: Id) {
        self.inner.frame.interaction.focused = Some(id);
    }

    pub fn clear_focus(&mut self) {
        self.inner.frame.interaction.focused = None;
    }

    pub fn pointer_position(&self) -> Option<Point<C::Scalar>> {
        self.inner.frame.interaction.position
    }

    /// this frame's input, the same for every widget
    pub fn input(&self) -> Input<C::Scalar> {
        self.inner.frame.input
    }

    pub fn screen(&self) -> Rect<C::Scalar> {
        self.inner.frame.screen
    }

    pub fn time(&self) -> Duration {
        self.inner.frame.time
    }

    pub fn animate(&mut self, id: Id, target: f32, duration: Duration, easing: Easing) -> f32 {
        let frame = &mut *self.inner.frame;
        animation::AnimationState::update(&mut frame.animations, id, target, |animation| {
            animation.advance(target, duration, easing, frame.time)
        })
    }

    pub fn animate_loop(&mut self, id: Id, duration: Duration, easing: Easing) -> f32 {
        let frame = &mut *self.inner.frame;
        animation::AnimationState::update(&mut frame.animations, id, 0.0, |animation| {
            animation.advance_loop(duration, easing, frame.time)
        })
    }

    pub fn timer(&mut self, id: Id, duration: Duration) -> bool {
        let frame = &mut *self.inner.frame;
        timer::TimerState::update(&mut frame.timers, id, duration, None, frame.time)
    }

    pub fn timer_loop(&mut self, id: Id, duration: Duration) -> bool {
        assert!(!duration.is_zero(), "looping timer duration must be nonzero");
        let frame = &mut *self.inner.frame;
        timer::TimerState::update(&mut frame.timers, id, duration, Some(duration), frame.time)
    }

    pub fn request_frame(&mut self) {
        self.inner.frame.request_frame();
    }
}

impl<'ui, C: Context, I: 'static> From<Ui<'ui, C, state::Child<I>>> for Ui<'ui, C> {
    fn from(ui: Ui<'ui, C, state::Child<I>>) -> Self {
        Self {
            inner: ui.inner,
            marker: PhantomData,
        }
    }
}

impl<C: Context, A: Atom<C>> Widget<C, state::Node> for AtomWidget<A> {
    type Response = ();

    fn build(self, ui: Ui<'_, C, state::Node>) {
        atom::insert(ui.inner.frame, ui.inner.node, self.0);
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
    Widget(Id),
    /// the frame root
    Root,
}

impl From<Id> for NodeTarget {
    fn from(id: Id) -> Self {
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
    scratch: &'ui BumpArena,
    node: NodeId,
    next_child: u32,
    owns_node: bool,
}

impl<C: Context> UiInner<'_, C> {
    fn set_widget_id(&mut self, id: Id) {
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
    fn new(frame: &'ui mut Frame<C>, context: &'ui mut C, scratch: &'ui BumpArena, node: NodeId) -> Self {
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
        self.frame.nodes[self.node.index()].subtree_end = (self.frame.nodes.len() - 1) as u32;
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
