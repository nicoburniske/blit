use std::marker::PhantomData;

#[cfg(debug_assertions)]
use super::LayoutState;
use super::{Frame, NodeId, StoredNode};
use crate::{
    TransitionProperties,
    arena::{DataArena, DataId},
    geometry::{Constraints, Point, Size},
    layout::{Axis, Layout, LayoutResolution, Sizing},
};

/// context for measuring and positioning a layout's children
pub struct LayoutCx<'a, C, I> {
    frame: &'a mut Frame<C>,
    data: &'a DataArena,
    context: &'a mut C,
    node: NodeId,
    nodes: *const StoredNode,
    default_item: DataId,
    item: PhantomData<fn() -> I>,
    first_child: NodeId,
    children_end: u32,
    offset: Point,
}

impl<'a, C, I: 'static> LayoutCx<'a, C, I> {
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
    ///
    /// children without explicit items share their layout type's default item
    #[inline]
    pub fn item(&self, child: NodeId) -> &'a I {
        self.assert_child(child);
        let id = self.frame.nodes[child.index()].item;
        self.data.load(if id.offset().is_some() {
            id
        } else {
            self.default_item
        })
    }

    /// applies layout resolution and any animated override to `sizing`
    #[inline]
    pub fn resolve_sizing(&self, child: NodeId, axis: Axis, sizing: Sizing) -> Sizing {
        let (width, height) = self.size_overrides(child);
        let extent = match axis {
            Axis::Horizontal => width,
            Axis::Vertical => height,
        };
        if let Some(extent) = extent {
            Sizing::fixed(extent)
        } else {
            self.frame.layout_resolution.sizing(axis, sizing)
        }
    }

    /// lays out `child` and returns its size
    ///
    /// animated size overrides replace the corresponding constraint axes
    ///
    /// repeating this recomputes its subtree and requires positioning it again
    pub fn layout_child(&mut self, child: NodeId, mut constraints: Constraints) -> Size {
        let (width, height) = self.size_overrides(child);
        if let Some(width) = width {
            constraints.min.width = width;
            constraints.max.width = width;
        }
        if let Some(height) = height {
            constraints.min.height = height;
            constraints.max.height = height;
        }
        let size = self
            .frame
            .layout_node(self.data, child, self.context, constraints);
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
            self.frame.target_sizes[child.index()].size
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

    /// requests another frame when layout changes cached geometry
    pub fn request_frame(&mut self) {
        self.frame.request_frame();
    }

    /// accesses context resources during layout
    #[inline]
    pub fn context(&mut self) -> &mut C {
        self.context
    }

    /// sets a child's paint order among its visual siblings
    #[inline]
    pub fn set_child_z_index(&mut self, child: NodeId, z_index: i16) {
        #[cfg(debug_assertions)]
        self.assert_child(child);
        self.frame.nodes[child.index()].z_index = z_index;
    }
}

impl<C, I: 'static> LayoutCx<'_, C, I> {
    #[inline]
    fn size_overrides(&self, child: NodeId) -> (Option<f32>, Option<f32>) {
        self.assert_child(child);
        if self.frame.target_sizes.is_empty() {
            return (None, None);
        }
        let current = self.frame.nodes[child.index()].area.size();
        let target = self.frame.target_sizes[child.index()];
        let res = self.frame.layout_resolution;
        (
            target
                .properties
                .intersects(TransitionProperties::WIDTH)
                .then(|| res.extent(Axis::Horizontal, current.width)),
            target
                .properties
                .intersects(TransitionProperties::HEIGHT)
                .then(|| res.extent(Axis::Vertical, current.height)),
        )
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

pub fn run<C, L: Layout<C>>(
    data: &DataArena,
    frame: &mut Frame<C>,
    node: NodeId,
    context: &mut C,
    id: DataId,
    constraints: Constraints,
) -> Size {
    let layout = data.load::<L>(id);
    let nodes = frame.nodes.as_ptr();
    let stored = frame.nodes[node.index()].layout.index().unwrap();
    let kind = frame.layouts[stored].kind as usize;
    let default_item = frame.layout_kinds[kind].default_item;
    let first_child = frame.node_id(node.index() + 1);
    let children_end = frame.nodes[node.index()].subtree_end;
    let offset = frame.layout_offset(node);
    let mut cx = LayoutCx {
        frame,
        data,
        context,
        node,
        nodes,
        default_item,
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
