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
    arena::{DataArena, DataId},
    geometry::{Constraints, Point, Rect, Sides, Size},
    input::Input,
    interact::{Interaction, Sense, WidgetId},
    layout::{Axis, Layout, LayoutResolution, Sizing},
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

    pub fn clip<X: Clip<C>>(mut self, clip: X) -> Self {
        let node = self.inner.node;
        let frame = self.inner.frame_mut();
        let clip = frame.store_clip(clip);
        frame.nodes[node.index()].clip = clip;
        self
    }

    /// names this node for interaction, geometry and references
    pub fn widget_id(mut self, id: WidgetId) -> Self {
        let node = self.inner.node;
        let frame = self.inner.frame_mut();
        if let Some(previous) = frame.nodes[node.index()].widget_id.replace(id) {
            // release the old name so another node can claim it
            *frame.named_nodes.get_mut(&previous).unwrap() = None;
        }
        assert!(
            frame.named_nodes.insert(id, Some(node)).flatten().is_none(),
            "widget ids must identify unique nodes"
        );
        self
    }

    pub fn hit(self, hit: Sides) -> Self {
        let node = self.inner.node;
        self.inner.frame.geometry_mut(node).hit = hit;
        self
    }

    pub fn transition(self, transition: Transition) -> Self {
        let node = self.inner.node;
        self.inner.frame.geometry_mut(node).transition = Some(transition);
        self
    }

    /// selects the parent for stacking, clipping and absolute sizing
    ///
    /// named targets must already be registered. positioning stays with its anchor.
    pub fn parent(mut self, target: impl Into<NodeTarget>) -> Self {
        let node = self.inner.node;
        let frame = self.inner.frame_mut();
        let parent = frame.resolve_target(node, target.into());
        frame.nodes[node.index()].visual_parent = parent;
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
                owns_node: false,
            },
            marker: PhantomData,
        })
    }
}

impl<'ui, C> Ui<'ui, C, state::Build> {
    /// builds a widget in this node
    pub fn build<W: Widget<C>>(self, widget: W) -> W::Response {
        widget.build(self)
    }

    /// establishes the current node's layout
    pub fn layout<L: Layout<C>>(self, layout: L) -> Ui<'ui, C, state::Open<L>> {
        let Ui { mut inner, .. } = self;
        let node = inner.node;
        let frame = inner.frame_mut();
        let layout = frame.store_layout(layout);
        frame.nodes[node.index()].layout = layout;
        Ui {
            inner,
            marker: PhantomData,
        }
    }
}

impl<'ui, C, I: 'static> Ui<'ui, C, state::Child<I>> {
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
    pub fn layout<L: Layout<C>>(self, layout: L) -> Ui<'ui, C, state::Open<L>> {
        let ui: Ui<'_, C> = Ui {
            inner: self.inner,
            marker: PhantomData,
        };
        ui.layout(layout)
    }
}

impl<'ui, C, L> Ui<'ui, C, state::Open<L>>
where
    L: Layout<C>,
    L::Item: Default,
{
    /// creates a child with this layout's shared default item
    #[inline]
    pub fn child(&mut self) -> Ui<'_, C, state::Child<L::Item>> {
        let node = self
            .inner
            .frame
            .push_child::<L::Item>(NewChild::Default(layout::store_default::<L::Item>));
        Ui::new(&mut *self.inner.frame, &mut *self.inner.context, node)
    }
}

impl<'ui, C, L: Layout<C>> Ui<'ui, C, state::Open<L>> {
    pub fn offset(mut self, offset: Point) -> Self {
        let node = self.inner.node;
        let frame = self.inner.frame_mut();
        let layout = frame.nodes[node.index()].layout.index().unwrap();
        frame.layouts[layout].offset = offset;
        self
    }

    /// creates a child with an explicit layout item
    /// required when layout item doesn't implement Default
    #[inline]
    pub fn child_item(&mut self, item: L::Item) -> Ui<'_, C, state::Child<L::Item>> {
        let node = self.inner.frame.push_child(NewChild::Item(item));
        Ui::new(&mut *self.inner.frame, &mut *self.inner.context, node)
    }

    /// creates an absolutely positioned child that bypasses this layout
    pub fn absolute(&mut self, absolute: Absolute) -> Ui<'_, C> {
        let node = self.inner.frame.push_node();
        let frame = self.inner.frame_mut();
        frame.set_absolute(node, absolute);
        frame.current_parent = Some(node);
        Ui::new(&mut *self.inner.frame, &mut *self.inner.context, node)
    }
}

impl<C, S> Ui<'_, C, S> {
    pub fn geometry(&self, id: WidgetId) -> Option<Rect> {
        self.inner.frame.geometry(id)
    }

    pub fn interact(&mut self, id: WidgetId, sense: Sense) -> Interaction {
        let frame = self.inner.frame_mut();
        let interaction = frame.interaction.response(id, sense);
        if interaction.activated || interaction.deactivated || interaction.clicked {
            frame.request_frame();
        }
        interaction
    }

    pub fn input(&self) -> &Input {
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
        let frame = self.inner.frame_mut();
        if frame.interaction.focus(id) {
            frame.request_frame();
        }
    }

    pub fn clear_focus(&mut self) {
        let frame = self.inner.frame_mut();
        if frame.interaction.clear_focus() {
            frame.request_frame();
        }
    }

    pub fn pointer_position(&self) -> Option<Point> {
        self.inner.frame.interaction.pointer_position()
    }

    pub fn screen(&self) -> Rect {
        self.inner.frame.screen
    }

    /// returns the frame's layout resolution
    pub fn layout_resolution(&self) -> LayoutResolution {
        self.inner.frame.layout_resolution
    }

    pub fn time(&self) -> Duration {
        self.inner.frame.time
    }

    pub fn animate(
        &mut self,
        id: WidgetId,
        target: f32,
        duration: Duration,
        easing: Easing,
    ) -> f32 {
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
        assert!(
            !duration.is_zero(),
            "looping timer duration must be nonzero"
        );
        let frame = self.inner.frame_mut();
        timer::TimerState::update(&mut frame.timers, id, duration, Some(duration), frame.time)
    }

    pub fn request_frame(&mut self) {
        self.inner.frame.request_frame();
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

/// placement and sizing of a child outside its parent layout
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Absolute {
    pub target: NodeTarget,
    pub target_anchor: Anchor,
    pub child_anchor: Anchor,
    pub offset: Point,
    pub width: Sizing,
    pub height: Sizing,
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
            target: NodeTarget::Parent,
            target_anchor: Anchor::TopLeft,
            child_anchor: Anchor::TopLeft,
            offset: Point::new(x, y),
            width: Sizing::fit(),
            height: Sizing::fit(),
        }
    }

    pub const fn screen(x: f32, y: f32) -> Self {
        Self {
            target: NodeTarget::Root,
            ..Self::at(x, y)
        }
    }

    pub const fn attach(target: Anchor, child: Anchor) -> Self {
        Self::at(0.0, 0.0).anchors(target, child)
    }

    /// anchors to an earlier node by id or registered widget name
    pub fn relative_to(mut self, target: impl Into<NodeTarget>) -> Self {
        self.target = target.into();
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

    pub const fn width(mut self, width: Sizing) -> Self {
        self.width = width;
        self
    }

    pub const fn height(mut self, height: Sizing) -> Self {
        self.height = height;
        self
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
    owns_node: bool,
}

impl<C> UiInner<'_, C> {
    #[inline]
    fn frame_mut(&mut self) -> &mut Frame<C> {
        self.frame
    }
}

impl<'ui, C, S> Ui<'ui, C, S> {
    fn new(frame: &'ui mut Frame<C>, context: &'ui mut C, node: NodeId) -> Self {
        Self {
            inner: UiInner {
                frame,
                context,
                node,
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
