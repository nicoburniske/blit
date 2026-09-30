use std::marker::PhantomData;

#[cfg(debug_assertions)]
use super::LayoutState;
use super::{Frame, NodeId, StoredNode};
use crate::{
    IntrinsicQuery, IntrinsicSize, TransitionProperties,
    arena::{DataArena, DataId},
    geometry::{Constraints, LogicalPoint, LogicalSize},
    layout::Layout,
};

/// access to child items and intrinsic queries without changing geometry
pub struct MeasureCx<'a, C, I> {
    frame: &'a mut Frame<C>,
    data: &'a DataArena,
    context: &'a mut C,
    node: NodeId,
    nodes: *const StoredNode,
    default_item: &'a I,
    first_child: NodeId,
    children_end: u32,
    scratch_start: usize,
}

impl<'a, C, I: 'static> MeasureCx<'a, C, I> {
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// iterates direct flow children with their typed items
    #[inline]
    pub fn children(&self) -> Children<'a, I> {
        Children {
            nodes: self.nodes,
            data: self.data,
            default_item: self.default_item,
            next: self.first_child,
            end: self.children_end,
        }
    }

    #[inline]
    pub fn intrinsic(&mut self, child: NodeId, query: IntrinsicQuery) -> IntrinsicSize {
        #[cfg(debug_assertions)]
        self.assert_child(child);
        self.frame.intrinsic_node(self.data, child, self.context, query)
    }

    pub fn context(&mut self) -> &mut C {
        self.context
    }

    pub fn scratch<T: Copy + 'static>(&mut self, len: usize, value: T) -> ScratchSlice<'a, T> {
        ScratchSlice {
            data: self.frame.scratch.store_slice(len, value),
            len,
            marker: PhantomData,
        }
    }

    pub fn scratch_mut<'s, T: Copy + 'static>(&'s self, slice: &'s mut ScratchSlice<'a, T>) -> &'s mut [T] {
        // safety: the unique handle grants exclusive access and the context borrow prevents relocation
        unsafe { &mut *self.frame.scratch.slice_ptr(slice.data, slice.len) }
    }

    #[cfg(debug_assertions)]
    #[track_caller]
    fn assert_child(&self, child: NodeId) {
        let stored = &self.frame.nodes[child.index()];
        assert!(
            child != self.node && stored.relative == self.node && !stored.out_of_flow,
            "layout can only access direct flow children"
        );
    }
}

impl<C, I> Drop for MeasureCx<'_, C, I> {
    fn drop(&mut self) {
        self.frame.scratch.rewind(self.scratch_start);
    }
}

/// adds geometry access and child placement to MeasureCx
pub struct LayoutCx<'a, C, I> {
    measure: MeasureCx<'a, C, I>,
    offset: LogicalPoint,
}

impl<C, I: 'static> LayoutCx<'_, C, I> {
    pub fn relative(&self) -> NodeId {
        self.measure.frame.nodes[self.measure.node.index()].relative
    }

    pub fn parent(&self) -> NodeId {
        self.measure.frame.nodes[self.measure.node.index()].parent
    }

    /// returns animated width and height overrides for `node`
    #[inline]
    pub fn size_overrides(&self, node: NodeId) -> (Option<f32>, Option<f32>) {
        if self.measure.frame.target_sizes.is_empty() {
            return (None, None);
        }
        let index = node.index();
        let current = self.measure.frame.nodes[index].area.size();
        let target = self.measure.frame.target_sizes[index];
        (
            target
                .properties
                .intersects(TransitionProperties::WIDTH)
                .then_some(current.width),
            target
                .properties
                .intersects(TransitionProperties::HEIGHT)
                .then_some(current.height),
        )
    }

    /// lays out `child` and returns its size
    ///
    /// animated size overrides replace the corresponding constraint axes
    ///
    /// repeating this recomputes its subtree and requires positioning it again
    #[inline]
    pub fn layout_child(&mut self, child: NodeId, constraints: Constraints) -> LogicalSize {
        #[cfg(debug_assertions)]
        self.assert_child(child);
        let size = self
            .measure
            .frame
            .layout_node(self.measure.data, child, self.measure.context, constraints);
        #[cfg(debug_assertions)]
        {
            self.measure.frame.nodes[child.index()].layout_state = LayoutState::Laid;
        }
        size
    }

    /// returns a resolved node's size
    ///
    /// this node's size is unresolved until its layout returns
    #[inline]
    pub fn size(&self, node: NodeId) -> LogicalSize {
        #[cfg(debug_assertions)]
        {
            let stored = &self.measure.frame.nodes[node.index()];
            if stored.relative == self.measure.node && !stored.out_of_flow && node != self.measure.node {
                assert_ne!(
                    stored.layout_state,
                    LayoutState::Unlaid,
                    "child size is unavailable before layout"
                );
            }
        }
        self.measure.frame.nodes[node.index()].area.size()
    }

    /// returns a node's frame target size for structural layout decisions
    ///
    /// during animated replay this preserves the first layout result while
    /// [`Self::size`] follows the animation. otherwise they match.
    #[inline]
    pub fn target_size(&self, node: NodeId) -> LogicalSize {
        let current = self.size(node);
        if !self.measure.frame.target_sizes.is_empty() {
            self.measure.frame.target_sizes[node.index()].size
        } else {
            current
        }
    }

    /// positions this node against its reference or a flow child against this layout
    #[inline]
    pub fn set_position(&mut self, node: NodeId, position: LogicalPoint) {
        let offset = if node == self.measure.node {
            LogicalPoint::ZERO
        } else {
            #[cfg(debug_assertions)]
            {
                self.assert_child(node);
                assert_ne!(
                    self.measure.frame.nodes[node.index()].layout_state,
                    LayoutState::Unlaid,
                    "child must be laid out before positioning"
                );
                self.measure.frame.nodes[node.index()].layout_state = LayoutState::Positioned;
            }
            self.offset
        };
        let area = &mut self.measure.frame.nodes[node.index()].area;
        area.x = position.x + offset.x;
        area.y = position.y + offset.y;
    }

    /// requests another frame when layout changes cached geometry
    pub fn request_frame(&mut self) {
        self.measure.frame.request_frame();
    }

    /// sets a child's paint order among its visual siblings
    #[inline]
    pub fn set_child_z_index(&mut self, child: NodeId, z_index: i16) {
        #[cfg(debug_assertions)]
        self.assert_child(child);
        self.measure.frame.nodes[child.index()].z_index = z_index;
    }
}

impl<'a, C, I> std::ops::Deref for LayoutCx<'a, C, I> {
    type Target = MeasureCx<'a, C, I>;

    fn deref(&self) -> &Self::Target {
        &self.measure
    }
}

impl<C, I> std::ops::DerefMut for LayoutCx<'_, C, I> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.measure
    }
}

/// scratch handles cannot escape their layout callback
pub struct ScratchSlice<'a, T> {
    data: DataId,
    len: usize,
    marker: PhantomData<fn(&'a mut ()) -> &'a mut T>,
}

pub struct Child<'a, I> {
    pub id: NodeId,
    pub item: &'a I,
}

impl<I> Copy for Child<'_, I> {}
impl<I> Clone for Child<'_, I> {
    fn clone(&self) -> Self {
        *self
    }
}

/// iterator over direct flow children
pub struct Children<'a, I> {
    nodes: *const StoredNode,
    data: &'a DataArena,
    default_item: &'a I,
    next: NodeId,
    end: u32,
}

impl<I> Copy for Children<'_, I> {}
impl<I> Clone for Children<'_, I> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, I: 'static> Iterator for Children<'a, I> {
    type Item = Child<'a, I>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while self.next.value <= self.end {
            let node = self.next;
            // safety: node storage is frozen while layout and intrinsic queries run
            let stored = unsafe { &*self.nodes.add(node.index()) };
            self.next.value = stored.subtree_end + 1;
            if !stored.out_of_flow {
                let item = if stored.item.offset().is_some() {
                    // safety: the typed builder stores this parent layout's item type
                    unsafe { self.data.load_unchecked::<I>(stored.item) }
                } else {
                    self.default_item
                };
                return Some(Child { id: node, item });
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
) -> LogicalSize {
    // safety: layout registration pairs this dispatch with stored L values
    let layout = unsafe { data.load_unchecked::<L>(id) };
    let offset = frame.layout_offset(node);
    let mut cx = LayoutCx {
        measure: measure_cx(data, frame, node, context),
        offset,
    };
    #[cfg(debug_assertions)]
    for child in cx.children() {
        cx.measure.frame.nodes[child.id.index()].layout_state = LayoutState::Unlaid;
    }
    let size = layout.layout(&mut cx, constraints);
    #[cfg(debug_assertions)]
    for child in cx.children() {
        assert_eq!(
            cx.measure.frame.nodes[child.id.index()].layout_state,
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

pub fn intrinsic<C, L: Layout<C>>(
    data: &DataArena,
    frame: &mut Frame<C>,
    node: NodeId,
    context: &mut C,
    id: DataId,
    query: IntrinsicQuery,
) -> IntrinsicSize {
    // safety: layout registration pairs this dispatch with stored L values
    let layout = unsafe { data.load_unchecked::<L>(id) };
    let mut cx = measure_cx(data, frame, node, context);
    layout.intrinsic(&mut cx, query)
}

fn measure_cx<'a, C, I: 'static>(
    data: &'a DataArena,
    frame: &'a mut Frame<C>,
    node: NodeId,
    context: &'a mut C,
) -> MeasureCx<'a, C, I> {
    let nodes = frame.nodes.as_ptr();
    let stored = frame.nodes[node.index()].layout.index().unwrap();
    let kind = frame.layouts[stored].kind as usize;
    // safety: registration stores I as the default item for this dispatch
    let default_item = unsafe { data.load_unchecked::<I>(frame.layout_kinds[kind].default_item) };
    let first_child = NodeId::new(node.index() + 1);
    let children_end = frame.nodes[node.index()].subtree_end;
    let scratch_start = frame.scratch.bytes();
    MeasureCx {
        frame,
        data,
        context,
        node,
        nodes,
        default_item,
        first_child,
        children_end,
        scratch_start,
    }
}
