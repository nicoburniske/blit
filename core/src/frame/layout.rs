//! frame layout storage

use std::mem::{MaybeUninit, align_of, size_of};

use super::*;

/// access to one container and its direct children during layout
pub struct LayoutCx<'a, I> {
    frame: &'a mut FrameGraph,
    renderer: &'a mut dyn Renderer,
    nodes: *const Node,
    node: NodeId,
    positioned: bool,
    item: std::marker::PhantomData<fn() -> I>,
    offset: LogicalPoint,
}

/// iterator over a layout container's in-flow direct children
#[derive(Clone)]
pub struct Children<'a> {
    nodes: *const Node,
    next: NodeId,
    end: u32,
    positioned: bool,
    marker: std::marker::PhantomData<&'a ()>,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while self.next.value <= self.end {
            let node = self.next;
            // safety: layout freezes node storage and subtree boundaries
            let stored = unsafe { &*self.nodes.add(node.index()) };
            self.next.value = stored.subtree_end + 1;
            if !self.positioned || !stored.layout.is_positioned() {
                return Some(node);
            }
        }
        None
    }
}

impl<'a, I: Copy + 'static> LayoutCx<'a, I> {
    /// the current size of a node
    #[inline]
    pub fn size(&self, node: NodeId) -> LogicalSize {
        self.assert_child(node);
        let area = self.frame.nodes[node.index()].area;
        LogicalSize {
            width: area.width,
            height: area.height,
        }
    }

    /// lays out a direct child under the supplied constraints
    pub fn layout_child(&mut self, node: NodeId, constraints: Constraints) -> LogicalSize {
        self.assert_child(node);
        self.frame
            .layout_node(node, constraints, self.renderer, self.positioned)
    }

    /// applies constraints to a child previously passed to [`Self::layout_child`]
    ///
    /// simple leaves reuse their current measured size
    /// containers and wrapped text recalculate their layout
    pub fn constrain_child(&mut self, node: NodeId, constraints: Constraints) -> LogicalSize {
        self.assert_child(node);
        if self.frame.stored_layout(node).is_none()
            && !matches!(
                self.frame.nodes[node.index()].content.decode(),
                ContentRef::Text(index) if self.frame.texts[index].options.wrap != TextWrap::None
            )
        {
            let size = constraints.constrain(self.size(node));
            self.frame.nodes[node.index()].area.width = size.width;
            self.frame.nodes[node.index()].area.height = size.height;
            size
        } else {
            self.frame
                .layout_node(node, constraints, self.renderer, self.positioned)
        }
    }

    /// sets a direct child's position relative to this container
    #[inline]
    pub fn set_position(&mut self, node: NodeId, position: LogicalPoint) {
        self.assert_child(node);
        self.frame.nodes[node.index()].area.x = position.x + self.offset.x;
        self.frame.nodes[node.index()].area.y = position.y + self.offset.y;
    }

    /// iterates over this container's in-flow direct children
    #[inline]
    pub fn children(&self) -> Children<'a> {
        Children {
            nodes: self.nodes,
            next: self.frame.node_id(self.node.index() + 1),
            end: self.frame.nodes[self.node.index()].subtree_end,
            positioned: self.positioned,
            marker: std::marker::PhantomData,
        }
    }

    /// metadata supplied by a direct child for this layout
    ///
    /// panics when the child was declared without metadata
    #[inline]
    pub fn item(&self, node: NodeId) -> I {
        self.assert_child(node);
        let offset = self.frame.nodes[node.index()]
            .layout_item
            .offset()
            .expect("layout item is missing. unit scope does not store metadata");
        self.frame.layout_data.load(offset)
    }

    /// the sizing requested by a child on an axis
    #[inline]
    pub fn sizing(&self, node: NodeId, axis: Axis) -> crate::container::Sizing {
        self.assert_child(node);
        self.frame.nodes[node.index()].sizing(axis)
    }

    /// the current size of a child on an axis
    #[inline]
    pub fn axis_size(&self, node: NodeId, axis: Axis) -> f32 {
        self.assert_child(node);
        self.frame.nodes[node.index()].size(axis)
    }

    /// sets a child's size on an axis without changing its position
    #[inline]
    pub fn set_size(&mut self, node: NodeId, axis: Axis, size: f32) {
        self.assert_child(node);
        self.frame.nodes[node.index()].set_size(axis, size)
    }

    /// sets a direct child's order among its paint siblings
    #[inline]
    pub fn set_z_index(&mut self, node: NodeId, z_index: i16) {
        self.assert_child(node);
        self.frame.nodes[node.index()].slot.z_index = z_index;
        self.frame.needs_paint_order |= z_index != 0;
    }

    #[inline]
    #[track_caller]
    fn assert_child(&self, node: NodeId) {
        assert_eq!(
            self.frame.nodes[node.index()].parent,
            self.node,
            "layout can only access direct children"
        );
    }
}

#[derive(Clone, Copy)]
pub struct StoredLayout {
    pub data_offset: u32,
    pub vtable: &'static LayoutVtable,
    pub offset: LogicalPoint,
}

pub type RunLayout = fn(
    NodeId,
    StoredLayout,
    &mut FrameGraph,
    Constraints,
    &mut dyn Renderer,
    positioned: bool,
) -> LogicalSize;

pub struct LayoutVtable {
    pub run: RunLayout,
    pub layout_type: fn() -> TypeId,
}

pub struct LayoutVtableFor<L>(std::marker::PhantomData<L>);

impl<L: Layout> LayoutVtableFor<L> {
    pub const VALUE: LayoutVtable = LayoutVtable {
        run: run_layout::<L>,
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
        assert!(align_of::<T>() <= align_of::<Word>());
        assert_eq!(offset % align_of::<T>(), 0);
        assert!(
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

fn run_layout<L: Layout>(
    node: NodeId,
    stored: StoredLayout,
    frame: &mut FrameGraph,
    constraints: Constraints,
    renderer: &mut dyn Renderer,
    positioned: bool,
) -> LogicalSize {
    debug_assert_eq!((stored.vtable.layout_type)(), TypeId::of::<L>());
    let layout: L = frame.layout_data.load(stored.data_offset as usize);
    let graph_nodes = frame.nodes.as_ptr();
    layout.layout(
        &mut LayoutCx {
            frame,
            renderer,
            nodes: graph_nodes,
            node,
            positioned,
            item: std::marker::PhantomData,
            offset: stored.offset,
        },
        constraints,
    )
}
