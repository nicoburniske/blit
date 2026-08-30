use super::{Container, Frame, NodeId, container};
use crate::{layout::Layout, leaf::Leaf, renderer::Renderer};

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
}

pub fn new<R: Renderer>(frame: &mut Frame<R>, parent: Option<NodeId>) -> Ui<'_, R> {
    Ui { frame, parent }
}
