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
    mem::size_of,
    num::NonZeroU16,
    ops::{Deref, DerefMut},
    ptr::NonNull,
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

pub struct Ui<R: Platform> {
    frame: NonNull<Frame<R>>,
    platform: NonNull<R>,
}

impl<R: Platform> Ui<R> {
    /// creates the frame's root layout node
    pub fn node<L: Layout<R>>(&mut self, layout: L) -> Node<'_, R, L> {
        let frame = self.frame_mut();
        let layout = frame.store_layout(layout);
        let node = frame.push_node(Some(layout));
        new_node(self, node)
    }

    pub fn layer(&mut self) -> LayerId {
        self.frame_mut().add_layer()
    }

    pub fn set_id(&mut self, node: NodeId, id: WidgetId) {
        self.frame_mut().set_id(node, id);
    }

    pub fn set_hit(&mut self, node: NodeId, hit: Sides) {
        self.frame_mut().set_hit(node, hit);
    }

    pub fn set_transition(&mut self, node: NodeId, transition: Transition) {
        self.frame_mut().set_transition(node, transition);
    }

    pub fn geometry(&self, id: WidgetId) -> Option<Rect> {
        self.frame().geometry(id)
    }

    pub fn interact(&mut self, id: WidgetId, sense: Sense) -> Interaction {
        let frame = self.frame_mut();
        let interaction = frame.interaction.response(id, sense);
        if interaction.activated || interaction.deactivated || interaction.clicked {
            frame.request_frame();
        }
        interaction
    }

    pub fn input(&self) -> &Input {
        &self.frame().input
    }

    /// accesses platform resources during frame construction
    ///
    /// drawing remains deferred to [`Atom`](crate::Atom) implementations
    pub fn platform(&mut self) -> &mut R {
        self.platform_mut()
    }

    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.frame().interaction.is_focused(id)
    }

    pub fn focus(&mut self, id: WidgetId) {
        let frame = self.frame_mut();
        if frame.interaction.focus(id) {
            frame.request_frame();
        }
    }

    pub fn clear_focus(&mut self) {
        let frame = self.frame_mut();
        if frame.interaction.clear_focus() {
            frame.request_frame();
        }
    }

    pub fn pointer_position(&self) -> Option<Point> {
        self.frame().interaction.pointer_position()
    }

    pub fn screen(&self) -> Rect {
        self.frame().screen
    }

    pub fn time(&self) -> Duration {
        self.frame().time
    }

    pub fn animate(
        &mut self,
        id: WidgetId,
        target: f32,
        duration: Duration,
        easing: Easing,
    ) -> f32 {
        let frame = self.frame_mut();
        let time = frame.time;
        animation::AnimationState::update(&mut frame.animations, id, target, |animation| {
            animation.advance(target, duration, easing, time)
        })
    }

    pub fn animate_loop(&mut self, id: WidgetId, duration: Duration, easing: Easing) -> f32 {
        let frame = self.frame_mut();
        let time = frame.time;
        animation::AnimationState::update(&mut frame.animations, id, 0.0, |animation| {
            animation.advance_loop(duration, easing, time)
        })
    }

    pub fn timer(&mut self, id: WidgetId, duration: Duration) -> bool {
        let frame = self.frame_mut();
        timer::TimerState::update(&mut frame.timers, id, duration, None, frame.time)
    }

    pub fn timer_loop(&mut self, id: WidgetId, duration: Duration) -> bool {
        assert!(
            !duration.is_zero(),
            "looping timer duration must be nonzero"
        );
        let frame = self.frame_mut();
        timer::TimerState::update(&mut frame.timers, id, duration, Some(duration), frame.time)
    }

    pub fn request_frame(&mut self) {
        self.frame_mut().request_frame();
    }
}

/// widget build context for the current frame node
pub struct Cx<'ui, R: Platform> {
    ui: &'ui mut Ui<R>,
    node: NodeId,
}

impl<'ui, R: Platform> Cx<'ui, R> {
    pub fn id(&self) -> NodeId {
        self.node
    }

    /// appends an atom to the current node
    pub fn atom<A: Atom<R>>(&mut self, atom: A) -> &mut Self {
        self.ui.frame_mut().push_atom(self.node, atom);
        self
    }

    /// establishes the current node's layout
    pub fn layout<L: Layout<R>>(self, layout: L) -> Node<'ui, R, L> {
        let frame = self.ui.frame_mut();
        assert!(
            frame.nodes[self.node.index()].layout.index().is_none(),
            "node already has a layout"
        );
        let layout = frame.store_layout(layout);
        frame.nodes[self.node.index()].layout = layout;
        new_node(self.ui, self.node)
    }
}

impl<R: Platform> Deref for Cx<'_, R> {
    type Target = Ui<R>;

    fn deref(&self) -> &Self::Target {
        self.ui
    }
}

impl<R: Platform> DerefMut for Cx<'_, R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ui
    }
}

// Ui erases frame and platform lifetimes so widget APIs only need &mut Ui<R>
// Frame::record keeps the only value inside the build callback
// Ui must remain non-Copy and non-Clone
// NonNull is dereferenced only through Ui borrows
impl<R: Platform> Ui<R> {
    fn build_node<W: Widget<R>>(&mut self, node: NodeId, widget: W) -> W::Response {
        let parent = self.frame().current_parent;
        self.frame_mut().current_parent = Some(node);
        let output = widget.build(Cx { ui: self, node });
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

include!("node.rs");
include!("graph.rs");
