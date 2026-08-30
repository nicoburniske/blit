use std::{any::TypeId, mem::size_of, ptr::NonNull, time::Duration};

pub mod container;
pub mod interaction;
pub mod layout;
pub mod paint;
pub mod position;
pub mod transition;

pub use container::{Absolute, Anchor, ChildCx, Container, LayerId, PositionTarget, Sizing, Slot};

use crate::{
    Clip, FrameInfo, Leaf, Platform, Widget,
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
    pub fn add<W: Widget<R>>(&mut self, widget: W) -> W::Response {
        widget.build(self)
    }

    pub fn add_leaf<L: Leaf<R>>(&mut self, leaf: L) -> NodeId {
        let frame = self.frame_mut();
        let base = frame.store_leaf(leaf);
        frame.push_node(Some(base), None)
    }

    pub fn layout<L: Layout<R>>(&mut self, layout: L) -> Container<'_, R, L> {
        let frame = self.frame_mut();
        let layout = frame.store_layout(layout);
        let node = frame.push_node(None, Some(layout));
        container::new(self, node)
    }

    pub fn layout_with<B, L>(&mut self, base: B, layout: L) -> Container<'_, R, L>
    where
        B: Widget<R, Response = NodeId>,
        L: Layout<R>,
    {
        let start = self.frame().nodes.len();
        let node = self.add(base);
        let frame = self.frame_mut();
        assert_eq!(node.index(), start, "layout base returned the wrong node");
        assert_eq!(
            frame.nodes.len(),
            start + 1,
            "layout base must add exactly one node"
        );
        assert!(
            frame.nodes[start].base.index().is_some(),
            "layout base did not add a leaf"
        );
        assert!(
            frame.nodes[start].layout.index().is_none(),
            "layout base already has a layout"
        );
        frame.nodes[start].layout = frame.store_layout(layout);
        container::new(self, node)
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
    /// drawing remains deferred to [`Leaf`](crate::Leaf) implementations
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
        crate::animation::AnimationState::update(&mut frame.animations, id, target, |animation| {
            animation.advance(target, duration, easing, time)
        })
    }

    pub fn animate_loop(&mut self, id: WidgetId, duration: Duration, easing: Easing) -> f32 {
        let frame = self.frame_mut();
        let time = frame.time;
        crate::animation::AnimationState::update(&mut frame.animations, id, 0.0, |animation| {
            animation.advance_loop(duration, easing, time)
        })
    }

    pub fn timer(&mut self, id: WidgetId, duration: Duration) -> bool {
        let frame = self.frame_mut();
        crate::timer::TimerState::update(&mut frame.timers, id, duration, None, frame.time)
    }

    pub fn timer_loop(&mut self, id: WidgetId, duration: Duration) -> bool {
        assert!(
            !duration.is_zero(),
            "looping timer duration must be nonzero"
        );
        let frame = self.frame_mut();
        crate::timer::TimerState::update(
            &mut frame.timers,
            id,
            duration,
            Some(duration),
            frame.time,
        )
    }

    pub fn request_frame(&mut self) {
        self.frame_mut().request_frame();
    }
}

// Ui erases frame and platform lifetimes so widget APIs only need &mut Ui<R>
// Frame::record keeps the only value inside the build callback
// Ui must remain non-Copy and non-Clone
// NonNull is dereferenced only through Ui borrows
impl<R: Platform> Ui<R> {
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

include!("graph.rs");
