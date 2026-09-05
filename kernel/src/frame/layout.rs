use std::marker::PhantomData;

#[cfg(debug_assertions)]
use super::LayoutState;
use super::{Frame, NodeId, StoredNode};
use crate::{
    Platform,
    arena::{DataArena, DataId},
    geometry::{Constraints, Point, Size},
    layout::{Layout, LayoutResolution},
};

/// context for measuring and positioning a layout's children
pub struct LayoutCx<'a, R: Platform, I> {
    frame: &'a mut Frame<R>,
    data: &'a DataArena,
    platform: &'a mut R,
    node: NodeId,
    nodes: *const StoredNode,
    item: PhantomData<fn() -> I>,
    first_child: NodeId,
    children_end: u32,
    offset: Point,
}

impl<'a, R: Platform, I: 'static> LayoutCx<'a, R, I> {
    /// iterates direct flow children in declaration order
    #[inline]
    pub fn children(&self) -> Children<'a> {
        Children {
            nodes: self.nodes,
            next: self.first_child,
            end: self.children_end,
            marker: PhantomData,
        }
    }

    /// returns this layout's item for `child`
    #[inline]
    pub fn item(&self, child: NodeId) -> &'a I {
        self.assert_child(child);
        self.data.load(self.frame.nodes[child.index()].item)
    }

    /// lays out `child` and returns its size
    ///
    /// repeating this recomputes its subtree and requires positioning it again
    pub fn layout_child(&mut self, child: NodeId, constraints: Constraints) -> Size {
        #[cfg(debug_assertions)]
        self.assert_child(child);
        let size = self
            .frame
            .layout_node(self.data, child, self.platform, constraints);
        #[cfg(debug_assertions)]
        {
            self.frame.nodes[child.index()].layout_state = LayoutState::Laid;
        }
        size
    }

    /// returns the size from the latest [`Self::layout_child`] call
    #[inline]
    pub fn child_size(&self, child: NodeId) -> Size {
        #[cfg(debug_assertions)]
        {
            self.assert_child(child);
            assert_ne!(
                self.frame.nodes[child.index()].layout_state,
                LayoutState::Unlaid,
                "child size is unavailable before layout"
            );
        }
        self.frame.nodes[child.index()].area.size()
    }

    /// returns the child's frame target size for structural layout decisions
    ///
    /// during animated replay this preserves the first layout result while
    /// [`Self::child_size`] follows the animation. otherwise they match.
    #[inline]
    pub fn target_child_size(&self, child: NodeId) -> Size {
        let current = self.child_size(child);
        if !self.frame.target_sizes.is_empty() {
            self.frame.target_sizes[child.index()]
        } else {
            current
        }
    }

    /// positions `child` in local coordinates after its final layout
    #[inline]
    pub fn set_child_position(&mut self, child: NodeId, position: Point) {
        #[cfg(debug_assertions)]
        {
            self.assert_child(child);
            assert_ne!(
                self.frame.nodes[child.index()].layout_state,
                LayoutState::Unlaid,
                "child must be laid out before positioning"
            );
            self.frame.nodes[child.index()].layout_state = LayoutState::Positioned;
        }
        let area = &mut self.frame.nodes[child.index()].area;
        area.x = position.x + self.offset.x;
        area.y = position.y + self.offset.y;
    }

    /// returns the resolution for adapting layout-owned physical lengths
    #[inline]
    pub fn resolution(&self) -> LayoutResolution {
        self.frame.layout_resolution
    }

    /// accesses platform resources during layout
    #[inline]
    pub fn platform(&mut self) -> &mut R {
        self.platform
    }

    /// sets a child's paint order within its layer
    #[inline]
    pub fn set_child_z_index(&mut self, child: NodeId, z_index: i16) {
        #[cfg(debug_assertions)]
        self.assert_child(child);
        self.frame.nodes[child.index()].z_index = z_index;
        self.frame.needs_paint_order |= z_index != 0;
    }

    #[track_caller]
    fn assert_child(&self, child: NodeId) {
        let stored = &self.frame.nodes[child.index()];
        assert!(
            child != self.node && stored.parent == self.node && stored.positioned.index().is_none(),
            "layout can only access direct flow children"
        );
    }
}

/// iterator over direct flow children
#[derive(Clone, Copy)]
pub struct Children<'a> {
    nodes: *const StoredNode,
    next: NodeId,
    end: u32,
    marker: PhantomData<&'a StoredNode>,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while self.next.value <= self.end {
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
    let children_end = frame.nodes[node.index()].subtree_end;
    let offset = frame.layout_offset(node);
    let mut cx = LayoutCx {
        frame,
        data,
        platform,
        node,
        nodes,
        item: PhantomData,
        first_child,
        children_end,
        offset,
    };
    #[cfg(debug_assertions)]
    for child in cx.children() {
        cx.frame.nodes[child.index()].layout_state = LayoutState::Unlaid;
    }
    let size = layout.layout(&mut cx, constraints);
    #[cfg(debug_assertions)]
    for child in cx.children() {
        assert_eq!(
            cx.frame.nodes[child.index()].layout_state,
            LayoutState::Positioned,
            "layout did not lay out and position every child"
        );
    }
    debug_assert_eq!(
        size,
        constraints.constrain(size),
        "layout returned a size outside its constraints"
    );
    size
}

pub fn override_item<R: Platform, L: Layout<R>>(
    data: &mut DataArena,
    layout: DataId,
    item: DataId,
    width: Option<f32>,
    height: Option<f32>,
) -> bool {
    let layout = data.load::<L>(layout) as *const L;
    let item = data.load_mut::<L::Item>(item);
    // safety: the layout and item occupy disjoint arena storage
    unsafe { (&*layout).override_size(item, width, height) }
}
