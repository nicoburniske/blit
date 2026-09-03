use std::marker::PhantomData;

use super::{Frame, NodeId, StoredNode};
use crate::{
    Platform,
    arena::{DataArena, DataId},
    frame::Sizing,
    geometry::{Constraints, Point, Sides, Size},
    layout::{Axis, Layout, LayoutResolution},
};

pub struct LayoutCx<'a, R: Platform, I> {
    frame: &'a mut Frame<R>,
    data: &'a DataArena,
    platform: &'a mut R,
    node: NodeId,
    nodes: *const StoredNode,
    item: PhantomData<fn() -> I>,
    first_child: NodeId,
    children_end: usize,
    offset: Point,
    resolution: LayoutResolution,
}

impl<'a, R: Platform, I: Copy + 'static> LayoutCx<'a, R, I> {
    #[inline]
    pub fn children(&self) -> Children<'a> {
        Children {
            nodes: self.nodes,
            next: self.first_child,
            end: self.children_end,
            marker: PhantomData,
        }
    }

    pub fn item(&self, child: NodeId) -> I {
        self.assert_child(child);
        let data = self.frame.nodes[child.index()].item;
        *self.data.load(data)
    }

    pub fn measure_base(&mut self, constraints: Constraints) -> Size {
        self.frame
            .measure_base(self.data, self.node, self.platform, constraints)
    }

    pub fn layout_child(&mut self, child: NodeId, constraints: Constraints) -> Size {
        self.assert_child(child);
        self.frame
            .layout_node(self.data, child, self.platform, constraints)
    }

    pub fn constrain_child(&mut self, child: NodeId, constraints: Constraints) -> Size {
        self.assert_child(child);
        if self.frame.nodes[child.index()].layout.index().is_none()
            && !self.frame.measure_depends_on_constraints(child)
        {
            let size = constraints.constrain(self.frame.nodes[child.index()].area.size());
            self.frame.nodes[child.index()].area.width = size.width;
            self.frame.nodes[child.index()].area.height = size.height;
            size
        } else {
            self.frame
                .layout_node(self.data, child, self.platform, constraints)
        }
    }

    #[inline]
    pub fn size(&self, child: NodeId) -> Size {
        self.assert_child(child);
        self.frame.nodes[child.index()].area.size()
    }

    #[inline]
    pub fn set_position(&mut self, child: NodeId, position: Point) {
        self.assert_child(child);
        let area = &mut self.frame.nodes[child.index()].area;
        area.x = position.x + self.offset.x;
        area.y = position.y + self.offset.y;
    }

    #[inline]
    pub fn sizing(&self, child: NodeId, axis: Axis) -> Sizing {
        self.assert_child(child);
        match axis {
            Axis::Horizontal => self.frame.nodes[child.index()].place.width,
            Axis::Vertical => self.frame.nodes[child.index()].place.height,
        }
    }

    #[inline]
    pub fn resolve_extent(&self, axis: Axis, value: f32) -> f32 {
        self.resolution.extent(axis, value)
    }

    #[inline]
    pub fn resolve_sides(&self, sides: Sides) -> Sides {
        Sides {
            top: self.resolve_extent(Axis::Vertical, sides.top),
            right: self.resolve_extent(Axis::Horizontal, sides.right),
            bottom: self.resolve_extent(Axis::Vertical, sides.bottom),
            left: self.resolve_extent(Axis::Horizontal, sides.left),
        }
    }

    #[inline]
    pub fn axis_size(&self, child: NodeId, axis: Axis) -> f32 {
        self.assert_child(child);
        let area = self.frame.nodes[child.index()].area;
        match axis {
            Axis::Horizontal => area.width,
            Axis::Vertical => area.height,
        }
    }

    #[inline]
    pub fn set_size(&mut self, child: NodeId, axis: Axis, size: f32) {
        self.assert_child(child);
        let area = &mut self.frame.nodes[child.index()].area;
        match axis {
            Axis::Horizontal => area.width = size,
            Axis::Vertical => area.height = size,
        }
    }

    #[inline]
    pub fn set_z_index(&mut self, child: NodeId, z_index: i16) {
        self.assert_child(child);
        self.frame.nodes[child.index()].place.z_index = z_index;
        self.frame.needs_paint_order |= z_index != 0;
    }

    #[track_caller]
    fn assert_child(&self, child: NodeId) {
        let child = &self.frame.nodes[child.index()];
        assert_eq!(
            child.parent, self.node,
            "layout can only access direct children"
        );
        assert!(
            child.positioned.index().is_none(),
            "layout cannot access absolute children"
        );
    }
}

#[derive(Clone, Copy)]
pub struct Children<'a> {
    nodes: *const StoredNode,
    next: NodeId,
    end: usize,
    marker: PhantomData<&'a StoredNode>,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while self.next.index() <= self.end {
            let node = self.next;
            // safety: node storage is frozen while layout runs
            let stored = unsafe { &*self.nodes.add(node.index()) };
            self.next.value = stored.subtree_end + 1;
            if stored.positioned.index().is_none() {
                return Some(node);
            }
        }
        None
    }
}

pub fn run<R: Platform, L: Layout<R>>(
    data: &DataArena,
    frame: &mut Frame<R>,
    node: NodeId,
    platform: &mut R,
    id: DataId,
    constraints: Constraints,
) -> Size {
    let layout = data.load::<L>(id);
    let nodes = frame.nodes.as_ptr();
    let first_child = frame.node_id(node.index() + 1);
    let children_end = frame.nodes[node.index()].subtree_end as usize;
    let offset = frame.layout_offset(node);
    let resolution = frame.layout_resolution;
    layout.layout(
        &mut LayoutCx {
            frame,
            data,
            platform,
            node,
            nodes,
            item: PhantomData,
            first_child,
            children_end,
            offset,
            resolution,
        },
        constraints,
    )
}
