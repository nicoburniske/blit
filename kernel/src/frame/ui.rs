use std::time::Duration;

use super::{Container, Frame, LayerId, NodeId, container};
use crate::{
    animation::{Easing, Transition},
    geometry::{Point, Rect, Sides},
    input::Input,
    interact::{Interaction, Sense, WidgetId},
    layout::Layout,
    leaf::Leaf,
    platform::Platform,
};

pub struct Ui<'a, R: Platform> {
    frame: &'a mut Frame<R>,
    platform: &'a mut R,
    parent: Option<NodeId>,
}

impl<R: Platform> Ui<'_, R> {
    pub fn add<L: Leaf<R>>(&mut self, leaf: L) -> NodeId {
        let base = self.frame.store_leaf(leaf);
        self.frame.push_node(self.parent, Some(base), None)
    }

    pub fn layout<L: Layout<R>>(&mut self, layout: L) -> Container<'_, R, L> {
        let layout = self.frame.store_layout(layout);
        let node = self.frame.push_node(self.parent, None, Some(layout));
        container::new(self.frame, self.platform, node)
    }

    pub fn layout_with<B: Leaf<R>, L: Layout<R>>(
        &mut self,
        base: B,
        layout: L,
    ) -> Container<'_, R, L> {
        let base = self.frame.store_leaf(base);
        let layout = self.frame.store_layout(layout);
        let node = self.frame.push_node(self.parent, Some(base), Some(layout));
        container::new(self.frame, self.platform, node)
    }

    pub fn layer(&mut self) -> LayerId {
        let owner = self.parent.expect("layer declaration requires a container");
        self.frame.add_layer(owner)
    }

    pub fn set_id(&mut self, node: NodeId, id: WidgetId) {
        self.frame.set_id(node, id);
    }

    pub fn set_hit(&mut self, node: NodeId, hit: Sides) {
        self.frame.set_hit(node, hit);
    }

    pub fn set_transition(&mut self, node: NodeId, transition: Transition) {
        self.frame.set_transition(node, transition);
    }

    pub fn geometry(&self, id: WidgetId) -> Option<Rect> {
        self.frame.geometry(id)
    }

    pub fn interact(&mut self, id: WidgetId, sense: Sense) -> Interaction {
        let interaction = self.frame.interaction.response(id, sense);
        if interaction.activated || interaction.deactivated || interaction.clicked {
            self.frame.request_frame();
        }
        interaction
    }

    pub fn input(&self) -> &Input {
        &self.frame.input
    }

    /// accesses platform resources during frame construction
    ///
    /// drawing remains deferred to [`Leaf`](crate::Leaf) implementations
    pub fn platform(&mut self) -> &mut R {
        self.platform
    }

    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.frame.interaction.is_focused(id)
    }

    pub fn focus(&mut self, id: WidgetId) {
        if self.frame.interaction.focus(id) {
            self.frame.request_frame();
        }
    }

    pub fn clear_focus(&mut self) {
        if self.frame.interaction.clear_focus() {
            self.frame.request_frame();
        }
    }

    pub fn pointer_position(&self) -> Option<Point> {
        self.frame.interaction.pointer_position()
    }

    pub fn screen(&self) -> Rect {
        self.frame.screen
    }

    pub fn time(&self) -> Duration {
        self.frame.time
    }

    pub fn animate(
        &mut self,
        id: WidgetId,
        target: f32,
        duration: Duration,
        easing: Easing,
    ) -> f32 {
        let time = self.frame.time;
        crate::animation::AnimationState::update(
            &mut self.frame.animations,
            id,
            target,
            |animation| animation.advance(target, duration, easing, time),
        )
    }

    pub fn animate_loop(&mut self, id: WidgetId, duration: Duration, easing: Easing) -> f32 {
        let time = self.frame.time;
        crate::animation::AnimationState::update(&mut self.frame.animations, id, 0.0, |animation| {
            animation.advance_loop(duration, easing, time)
        })
    }

    pub fn timer(&mut self, id: WidgetId, duration: Duration) -> bool {
        crate::timer::TimerState::update(
            &mut self.frame.timers,
            id,
            duration,
            None,
            self.frame.time,
        )
    }

    pub fn timer_loop(&mut self, id: WidgetId, duration: Duration) -> bool {
        assert!(
            !duration.is_zero(),
            "looping timer duration must be nonzero"
        );
        crate::timer::TimerState::update(
            &mut self.frame.timers,
            id,
            duration,
            Some(duration),
            self.frame.time,
        )
    }

    pub fn request_frame(&mut self) {
        self.frame.request_frame();
    }
}

pub fn new<'a, R: Platform>(
    frame: &'a mut Frame<R>,
    platform: &'a mut R,
    parent: Option<NodeId>,
) -> Ui<'a, R> {
    Ui {
        frame,
        platform,
        parent,
    }
}
