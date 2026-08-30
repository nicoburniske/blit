use std::marker::PhantomData;

use super::{Frame, NodeId, Ui, ui};
use crate::{clip::Clip, layout::Layout, renderer::Renderer};

pub struct Container<'a, R, L>
where
    R: Renderer,
    L: Layout<R>,
{
    frame: &'a mut Frame<R>,
    node: NodeId,
    marker: PhantomData<L>,
}

impl<R: Renderer, L: Layout<R>> Container<'_, R, L> {
    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn clip<C: Clip<R>>(self, clip: C) -> Self {
        assert!(
            self.frame.nodes[self.node.index()].clip.is_none(),
            "layout already has a clip"
        );
        let clip = self.frame.store_clip(clip);
        self.frame.nodes[self.node.index()].clip = Some(clip);
        self
    }

    pub fn add<O>(&mut self, item: L::Item, child: impl FnOnce(Ui<'_, R>) -> O) -> O {
        let start = self.frame.nodes.len();
        let output = child(ui::new(self.frame, Some(self.node)));
        let end = self.frame.nodes.len();
        assert!(end > start, "layout child did not add a node");

        let child = NodeId(start as u32);
        assert_eq!(
            self.frame.nodes[child.index()].parent,
            Some(self.node),
            "layout child was added outside its parent"
        );
        assert_eq!(
            self.frame.nodes[child.index()].subtree_end as usize + 1,
            end,
            "a layout item must contain exactly one root"
        );
        let data = self.frame.data.store(item);
        self.frame.nodes[child.index()].item = Some(data);
        output
    }
}

impl<R: Renderer, L: Layout<R>> Drop for Container<'_, R, L> {
    fn drop(&mut self) {
        self.frame.nodes[self.node.index()].subtree_end =
            u32::try_from(self.frame.nodes.len() - 1).expect("too many frame nodes");
    }
}

pub fn new<R: Renderer, L: Layout<R>>(frame: &mut Frame<R>, node: NodeId) -> Container<'_, R, L> {
    Container {
        frame,
        node,
        marker: PhantomData,
    }
}
