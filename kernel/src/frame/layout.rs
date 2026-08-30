use std::marker::PhantomData;

use super::{Frame, Node, NodeId};
use crate::{
    arena::DataId,
    geometry::{Constraints, Point, Size},
    layout::Layout,
    renderer::Renderer,
};

pub struct LayoutCx<'a, R: Renderer, I> {
    frame: &'a mut Frame<R>,
    renderer: &'a mut R,
    node: NodeId,
    nodes: *const Node,
    item: PhantomData<fn() -> I>,
}

impl<'a, R: Renderer, I: Copy + 'static> LayoutCx<'a, R, I> {
    pub fn children(&self) -> Children<'a> {
        let node = self.frame.nodes[self.node.index()];
        Children {
            nodes: self.nodes,
            next: self.node.index() + 1,
            end: node.subtree_end as usize,
            marker: PhantomData,
        }
    }

    pub fn item(&self, child: NodeId) -> I {
        self.assert_child(child);
        let data = self.frame.nodes[child.index()]
            .item
            .expect("layout item is missing");
        self.frame.data.load(data)
    }

    pub fn measure_base(&mut self, constraints: Constraints) -> Size {
        self.frame
            .measure_base(self.node, self.renderer, constraints)
    }

    pub fn layout_child(&mut self, child: NodeId, constraints: Constraints) -> Size {
        self.assert_child(child);
        self.frame.layout_node(child, self.renderer, constraints)
    }

    pub fn size(&self, child: NodeId) -> Size {
        self.assert_child(child);
        self.frame.nodes[child.index()].area.size()
    }

    pub fn set_position(&mut self, child: NodeId, position: Point) {
        self.assert_child(child);
        let area = &mut self.frame.nodes[child.index()].area;
        area.x = position.x;
        area.y = position.y;
    }

    pub fn set_z_index(&mut self, child: NodeId, z_index: i16) {
        self.assert_child(child);
        self.frame.nodes[child.index()].z_index = z_index;
        self.frame.needs_paint_order |= z_index != 0;
    }

    #[track_caller]
    fn assert_child(&self, child: NodeId) {
        assert_eq!(
            self.frame.nodes[child.index()].parent,
            Some(self.node),
            "layout can only access direct children"
        );
    }
}

#[derive(Clone, Copy)]
pub struct Children<'a> {
    nodes: *const Node,
    next: usize,
    end: usize,
    marker: PhantomData<&'a Node>,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next <= self.end {
            let node = NodeId(self.next as u32);
            // safety: node storage is frozen while layout runs
            let stored = unsafe { &*self.nodes.add(self.next) };
            self.next = stored.subtree_end as usize + 1;
            if stored.positioned.is_none() {
                return Some(node);
            }
        }
        None
    }
}

pub fn run<R: Renderer, L: Layout<R>>(
    frame: &mut Frame<R>,
    node: NodeId,
    renderer: &mut R,
    id: DataId,
    constraints: Constraints,
) -> Size {
    let layout = frame.data.load::<L>(id);
    let nodes = frame.nodes.as_ptr();
    layout.layout(
        LayoutCx {
            frame,
            renderer,
            node,
            nodes,
            item: PhantomData,
        },
        constraints,
    )
}
