//! frame layout storage

use std::mem::{MaybeUninit, align_of, size_of};

use super::*;

/// access to one container and its direct children during layout
pub struct LayoutCx<'a, I> {
    frame: &'a mut FrameGraph,
    nodes: *const Node,
    node: NodeId,
    positioned: bool,
    item: std::marker::PhantomData<fn() -> I>,
    offset: LogicalPoint,
}

/// iterator over a layout container's direct children
pub struct Children<'a> {
    nodes: *const Node,
    next: usize,
    end: usize,
    marker: std::marker::PhantomData<&'a ()>,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.end {
            return None;
        }
        let node = NodeId::new(self.next);
        // safety: layout freezes node storage and subtree boundaries
        self.next = unsafe { (*self.nodes.add(self.next)).subtree_end as usize };
        Some(node)
    }
}

impl<'a, I: Copy + 'static> LayoutCx<'a, I> {
    /// the container being laid out
    #[inline]
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// the container's current rectangle
    #[inline]
    pub fn rect(&self) -> LogicalRect {
        self.frame.nodes[self.node.index()].area
    }

    /// the current size of a node
    #[inline]
    pub fn size(&self, node: NodeId) -> LogicalSize {
        let area = self.frame.nodes[node.index()].area;
        LogicalSize {
            width: area.width,
            height: area.height,
        }
    }

    /// iterates over this container's direct children
    #[inline]
    pub fn children(&self) -> Children<'a> {
        Children {
            nodes: self.nodes,
            next: self.node.index() + 1,
            end: self.frame.nodes[self.node.index()].subtree_end as usize,
            marker: std::marker::PhantomData,
        }
    }

    /// whether a child participates in this container's layout
    #[inline]
    pub fn is_in_flow(&self, node: NodeId) -> bool {
        debug_assert_eq!(self.frame.nodes[node.index()].parent, self.node);
        !self.positioned || !self.frame.nodes[node.index()].layout.is_positioned()
    }

    /// metadata supplied by a direct child for this layout
    ///
    /// panics when the child was declared without metadata
    #[inline]
    pub fn item(&self, node: NodeId) -> I {
        assert_eq!(
            self.frame.nodes[node.index()].parent,
            self.node,
            "layout metadata is only available for direct children"
        );
        let offset = self.frame.nodes[node.index()]
            .layout_item
            .offset()
            .expect("layout item is missing. unit scope does not store metadata");
        self.frame.layout_data.load(offset)
    }

    /// the sizing requested by a child on an axis
    #[inline]
    pub fn sizing(&self, node: NodeId, axis: Axis) -> crate::container::Sizing {
        debug_assert_eq!(self.frame.nodes[node.index()].parent, self.node);
        self.frame.nodes[node.index()].sizing(axis)
    }

    /// the current size of a child on an axis
    #[inline]
    pub fn axis_size(&self, node: NodeId, axis: Axis) -> f32 {
        debug_assert_eq!(self.frame.nodes[node.index()].parent, self.node);
        self.frame.nodes[node.index()].size(axis)
    }

    /// sets a child's size on an axis without changing its position
    #[inline]
    pub fn set_size(&mut self, node: NodeId, axis: Axis, size: f32) {
        debug_assert_eq!(self.frame.nodes[node.index()].parent, self.node);
        self.frame.nodes[node.index()].set_size(axis, size)
    }

    /// sets a direct child's order among its paint siblings
    #[inline]
    pub fn set_z_index(&mut self, node: NodeId, z_index: i16) {
        assert_eq!(
            self.frame.nodes[node.index()].parent,
            self.node,
            "z-index can only be set for direct children"
        );
        self.frame.nodes[node.index()].slot.z_index = z_index;
        self.frame.needs_paint_order |= z_index != 0;
    }

    /// sets a child's position and size on an axis
    #[inline]
    pub fn set_axis(&mut self, node: NodeId, axis: Axis, position: f32, size: f32) {
        debug_assert_eq!(self.frame.nodes[node.index()].parent, self.node);
        let position = position
            + match axis {
                Axis::Horizontal => self.offset.x,
                Axis::Vertical => self.offset.y,
            };
        self.frame.nodes[node.index()].set_axis(axis, position, size)
    }
}

#[derive(Clone, Copy)]
pub struct StoredLayout {
    pub data_offset: u32,
    pub vtable: &'static LayoutVtable,
    pub offset: LogicalPoint,
}

pub type RunLayouts = unsafe fn(*const NodeId, usize, &mut FrameGraph, Axis, positioned: bool);

pub struct LayoutVtable {
    pub measure: RunLayouts,
    pub place: RunLayouts,
    pub layout_type: fn() -> TypeId,
}

pub struct LayoutVtableFor<L>(std::marker::PhantomData<L>);

impl<L: Layout> LayoutVtableFor<L> {
    pub const VALUE: LayoutVtable = LayoutVtable {
        measure: run_layout_batch::<L, false>,
        place: run_layout_batch::<L, true>,
        layout_type: TypeId::of::<L>,
    };
}

#[derive(Clone, Copy)]
pub struct PositionedLayout {
    pub layout: StoredLayout,
    pub offset: LogicalPoint,
    pub target: NodeId,
    pub target_anchor: Anchor,
    pub child_anchor: Anchor,
    pub uses_target_content_origin: bool,
}

#[repr(C, align(8))]
pub struct Word(MaybeUninit<[u8; 8]>);

#[derive(Default)]
pub struct DataArena {
    pub words: Vec<Word>,
    pub len: usize,
}

impl DataArena {
    pub fn store<T: Copy>(&mut self, value: T) -> u32 {
        const {
            assert!(
                align_of::<T>() <= align_of::<Word>(),
                "layout data alignment exceeds 8 bytes"
            );
        }
        let offset = self
            .len
            .checked_next_multiple_of(align_of::<T>())
            .expect("too much layout data in one frame");
        let end = offset
            .checked_add(size_of::<T>())
            .expect("too much layout data in one frame");
        self.words.resize_with(end.div_ceil(size_of::<Word>()), || {
            Word(MaybeUninit::uninit())
        });
        unsafe {
            self.words
                .as_mut_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<T>()
                .write(value)
        };
        self.len = end;
        u32::try_from(offset).expect("too much layout data in one frame")
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn load<T: Copy>(&self, offset: usize) -> T {
        debug_assert!(align_of::<T>() <= align_of::<Word>());
        debug_assert_eq!(offset % align_of::<T>(), 0);
        debug_assert!(
            offset
                .checked_add(size_of::<T>())
                .is_some_and(|end| end <= self.len)
        );
        // safety: store wrote an aligned T at this offset
        unsafe {
            self.words
                .as_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<T>()
                .read()
        }
    }

    pub fn heap_bytes(&self) -> usize {
        self.words.capacity() * size_of::<Word>()
    }
}

unsafe fn run_layout_batch<L: Layout, const PLACE: bool>(
    nodes: *const NodeId,
    len: usize,
    frame: &mut FrameGraph,
    axis: Axis,
    positioned: bool,
) {
    for index in 0..len {
        let index = if PLACE { index } else { len - index - 1 };
        // safety: the scheduler passes a frozen slice of layout node IDs
        let node = unsafe { nodes.add(index).read() };
        let stored = frame.stored_layout(node).unwrap();
        debug_assert_eq!((stored.vtable.layout_type)(), TypeId::of::<L>());
        let layout: L = frame.layout_data.load(stored.data_offset as usize);
        let graph_nodes = frame.nodes.as_ptr();
        let mut cx = LayoutCx {
            frame,
            nodes: graph_nodes,
            node,
            positioned,
            item: std::marker::PhantomData,
            offset: stored.offset,
        };
        if PLACE {
            if positioned && cx.frame.nodes[node.index()].layout.is_positioned() {
                cx.frame.resolve_positioned(node, axis);
            }
            layout.place(&mut cx, axis);
        } else if let Some(size) = layout.measure(&cx, axis) {
            cx.frame.nodes[node.index()].set_size(axis, size);
        }
    }
}
