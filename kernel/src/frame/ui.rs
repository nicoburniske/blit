use std::time::Duration;

use super::{Container, Frame, LayerId, NodeId, container};
use crate::{
    animation::Transition,
    geometry::{Point, Rect, Sides},
    input::Input,
    interact::{Interaction, Sense, WidgetId},
    layout::Layout,
    leaf::Leaf,
    renderer::Renderer,
};

pub struct Ui<'a, R: Renderer> {
    frame: &'a mut Frame<R>,
    parent: Option<NodeId>,
}

impl<R: Renderer> Ui<'_, R> {
    pub fn add<L: Leaf<R>>(&mut self, leaf: L) -> NodeId {
        let base = self.frame.store_leaf(leaf);
        self.frame.push_node(self.parent, Some(base), None)
    }

    pub fn layout<L: Layout<R>>(&mut self, layout: L) -> Container<'_, R, L> {
        let layout = self.frame.store_layout(layout);
        let node = self.frame.push_node(self.parent, None, Some(layout));
        container::new(self.frame, node)
    }

    pub fn layout_with<B: Leaf<R>, L: Layout<R>>(
        &mut self,
        base: B,
        layout: L,
    ) -> Container<'_, R, L> {
        let base = self.frame.store_leaf(base);
        let layout = self.frame.store_layout(layout);
        let node = self.frame.push_node(self.parent, Some(base), Some(layout));
        container::new(self.frame, node)
    }

    pub fn layer(&mut self) -> LayerId {
        let owner = self.parent.expect("layer declaration requires a container");
        self.frame.add_layer(owner)
    }

    pub fn set_id(&mut self, node: NodeId, id: WidgetId) {
        self.frame.nodes[node.index()].id = Some(id);
    }

    pub fn set_hit(&mut self, node: NodeId, hit: Sides) {
        self.frame.nodes[node.index()].hit = hit;
    }

    pub fn set_transition(&mut self, node: NodeId, transition: Transition) {
        self.frame.nodes[node.index()].transition = Some(transition);
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

    pub fn request_frame(&mut self) {
        self.frame.request_frame();
    }
}

pub fn new<R: Renderer>(frame: &mut Frame<R>, parent: Option<NodeId>) -> Ui<'_, R> {
    Ui { frame, parent }
}
