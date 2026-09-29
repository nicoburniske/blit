use std::marker::PhantomData;

#[cfg(debug_assertions)]
use super::LayoutState;
use super::{Frame, NodeId, StoredNode};
use crate::{
    TransitionProperties,
    arena::{DataArena, DataId},
    geometry::{Constraints, Point, Size},
    layout::Layout,
};

/// context for measuring and positioning a layout node and its flow children
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
    /// returns the node whose layout is running
    #[inline]
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// returns this node's layout parent
    #[inline]
    pub fn parent(&self) -> NodeId {
        self.frame.nodes[self.node.index()].parent
    }

    /// returns this node's paint and clipping parent
    #[inline]
    pub fn visual_parent(&self) -> NodeId {
        self.frame.nodes[self.node.index()].visual_parent
    }

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
        self.data
            .load(if id.offset().is_some() { id } else { self.default_item })
    }

    /// returns animated width and height overrides for `node`
    #[inline]
    pub fn size_overrides(&self, node: NodeId) -> (Option<f32>, Option<f32>) {
        if self.frame.target_sizes.is_empty() {
            return (None, None);
        }
        let index = node.index();
        let current = self.frame.nodes[index].area.size();
        let target = self.frame.target_sizes[index];
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
    pub fn layout_child(&mut self, child: NodeId, mut constraints: Constraints) -> Size {
        self.assert_child(child);
        let (width, height) = self.size_overrides(child);
        if let Some(width) = width {
            constraints.min.width = width;
            constraints.max.width = width;
        }
        if let Some(height) = height {
            constraints.min.height = height;
            constraints.max.height = height;
        }
        let size = self.frame.layout_node(self.data, child, self.context, constraints);
        #[cfg(debug_assertions)]
        {
            self.frame.nodes[child.index()].layout_state = LayoutState::Laid;
        }
        size
    }

    /// returns a resolved node's size
    ///
    /// this node's size is unresolved until its layout returns
    #[inline]
    pub fn size(&self, node: NodeId) -> Size {
        #[cfg(debug_assertions)]
        {
            let stored = &self.frame.nodes[node.index()];
            if stored.parent == self.node && !stored.out_of_flow && node != self.node {
                assert_ne!(
                    stored.layout_state,
                    LayoutState::Unlaid,
                    "child size is unavailable before layout"
                );
            }
        }
        self.frame.nodes[node.index()].area.size()
    }

    /// returns a node's frame target size for structural layout decisions
    ///
    /// during animated replay this preserves the first layout result while
    /// [`Self::size`] follows the animation. otherwise they match.
    #[inline]
    pub fn target_size(&self, node: NodeId) -> Size {
        let current = self.size(node);
        if !self.frame.target_sizes.is_empty() {
            self.frame.target_sizes[node.index()].size
        } else {
            current
        }
    }

    /// positions this node relative to its parent or a flow child relative to this layout
    #[inline]
    pub fn set_position(&mut self, node: NodeId, position: Point) {
        let offset = if node == self.node {
            Point::ZERO
        } else {
            #[cfg(debug_assertions)]
            {
                self.assert_child(node);
                assert_ne!(
                    self.frame.nodes[node.index()].layout_state,
                    LayoutState::Unlaid,
                    "child must be laid out before positioning"
                );
                self.frame.nodes[node.index()].layout_state = LayoutState::Positioned;
            }
            self.offset
        };
        let area = &mut self.frame.nodes[node.index()].area;
        area.x = position.x + offset.x;
        area.y = position.y + offset.y;
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
    #[track_caller]
    fn assert_child(&self, child: NodeId) {
        let stored = &self.frame.nodes[child.index()];
        assert!(
            child != self.node && stored.parent == self.node && !stored.out_of_flow,
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
            if !stored.out_of_flow {
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
    let first_child = NodeId::new(node.index() + 1);
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
