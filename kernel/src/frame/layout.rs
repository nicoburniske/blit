use std::marker::PhantomData;

use super::{Frame, NodeGeometry, NodeId, StoredNode};
use crate::{
    Context, Scalar, TransitionProperties,
    arena::{DataArena, DataId, Scratch},
    geometry::{Constraints, Point, Size},
    layout::Layout,
};

#[repr(C)]
pub struct LayoutCx<'a, C: Context, I = ()> {
    frame: &'a Frame<C>,
    data: &'a DataArena,
    geometry: &'a mut [NodeGeometry<C::Scalar>],
    context: &'a mut C,
    frame_requested: &'a mut bool,
    node: NodeId,
    default_item: DataId,
    item: PhantomData<fn() -> I>,
}

impl<'a, C: Context, I: 'static> LayoutCx<'a, C, I> {
    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn relative(&self) -> NodeId {
        self.frame.nodes[self.node.index()].relative
    }

    pub fn parent(&self) -> NodeId {
        self.frame.nodes[self.node.index()].visual_parent
    }

    pub fn size(&self, node: NodeId) -> Size<C::Scalar> {
        self.geometry[node.index()].area.size()
    }

    pub fn measure_atoms(&mut self, constraints: Constraints<C::Scalar>) -> Size<C::Scalar> {
        let node = self.node;
        self.cast::<()>().measure(node, constraints)
    }

    pub fn set_position(&mut self, position: Point<C::Scalar>) {
        let area = &mut self.geometry[self.node.index()].area;
        area.x = position.x;
        area.y = position.y;
    }

    #[inline]
    pub fn children(&self) -> Children<'a> {
        Children {
            nodes: &self.frame.nodes,
            next: NodeId::new(self.node.index() + 1),
            end: self.frame.nodes[self.node.index()].subtree_end,
        }
    }

    #[inline]
    pub fn item(&self, child: NodeId) -> &'a I {
        let id = self.assert_child(child).item;
        self.data
            .load(if id.offset().is_some() { id } else { self.default_item })
    }

    #[inline]
    pub fn layout_child(&mut self, child: NodeId, bounds: Constraints<C::Scalar>) -> Size<C::Scalar> {
        self.assert_child(child);
        self.cast::<()>().resolve(child, bounds)
    }

    pub fn target_size(&self, child: NodeId) -> Size<C::Scalar> {
        if self.frame.target_sizes.is_empty() {
            self.size(child)
        } else {
            self.frame.target_sizes[child.index()].size
        }
    }

    pub fn set_child_position(&mut self, child: NodeId, position: Point<C::Scalar>) {
        #[cfg(debug_assertions)]
        self.assert_child(child);
        let area = &mut self.geometry[child.index()].area;
        area.x = position.x;
        area.y = position.y;
    }

    pub fn request_frame(&mut self) {
        *self.frame_requested = true;
    }

    pub fn context(&mut self) -> &mut C {
        self.context
    }

    pub fn scratch<T: Copy>(&self, len: usize, value: T) -> Scratch<'a, T> {
        self.data.scratch(len, value)
    }

    pub fn set_child_z_index(&mut self, child: NodeId, z_index: i16) {
        #[cfg(debug_assertions)]
        self.assert_child(child);
        self.geometry[child.index()].z_index = z_index;
    }

    pub fn size_overrides(&self, child: NodeId) -> (Option<C::Scalar>, Option<C::Scalar>) {
        if self.frame.target_sizes.is_empty() {
            return (None, None);
        }
        let current = self.size(child);
        let target = self.frame.target_sizes[child.index()];
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

    #[track_caller]
    fn assert_child(&self, child: NodeId) -> &'a StoredNode {
        let stored = &self.frame.nodes[child.index()];
        assert!(
            child != self.node && stored.parent == self.node && !stored.out_of_flow,
            "layout can only access direct flow children"
        );
        stored
    }

    #[inline]
    fn cast<J>(&mut self) -> &mut LayoutCx<'a, C, J> {
        // safety: repr(C) changes only the item marker and preserves this borrow and lifetime
        unsafe { &mut *std::ptr::from_mut(self).cast::<LayoutCx<'a, C, J>>() }
    }
}

impl<'a, C: Context> LayoutCx<'a, C> {
    #[inline]
    fn measure(&mut self, node: NodeId, constraints: Constraints<C::Scalar>) -> Size<C::Scalar> {
        if constraints.min == constraints.max {
            return constraints.min;
        }
        let mut atom = self.frame.nodes[node.index()].first_atom;
        let mut size = Size::ZERO;
        while let Some(index) = atom.index() {
            let stored = self.frame.atoms[index];
            let measure = self.frame.atom_kinds[stored.kind as usize].measure;
            size = size.max(measure(self.data, stored.data, self.context, constraints));
            atom = stored.next;
        }
        constraints.constrain(size)
    }

    #[inline]
    fn resolve(&mut self, node: NodeId, mut bounds: Constraints<C::Scalar>) -> Size<C::Scalar> {
        let (width, height) = self.size_overrides(node);
        if let Some(width) = width {
            bounds.min.width = width;
            bounds.max.width = width;
        }
        if let Some(height) = height {
            bounds.min.height = height;
            bounds.max.height = height;
        }
        let stored = &self.frame.nodes[node.index()];
        let size = if stored.layout.offset().is_some() {
            let kind = &self.frame.layout_kinds[stored.layout_kind as usize];
            let mut cx = LayoutCx {
                frame: self.frame,
                data: self.data,
                geometry: self.geometry,
                context: self.context,
                frame_requested: self.frame_requested,
                node,
                default_item: kind.default_item,
                item: PhantomData,
            };
            (kind.run)(&mut cx, stored.layout, bounds)
        } else {
            self.measure(node, bounds)
        };
        debug_assert_eq!(
            size,
            bounds.constrain(size),
            "layout returned a size outside its constraints"
        );
        let area = &mut self.geometry[node.index()].area;
        area.width = size.width;
        area.height = size.height;
        size
    }
}

#[derive(Clone, Copy)]
pub struct Children<'a> {
    nodes: &'a [StoredNode],
    next: NodeId,
    end: u32,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    #[inline]
    fn next(&mut self) -> Option<NodeId> {
        while self.next.value <= self.end {
            let node = self.next;
            let stored = &self.nodes[node.index()];
            self.next.value = stored.subtree_end + 1;
            if !stored.out_of_flow {
                return Some(node);
            }
        }
        None
    }
}

pub fn resolve<C: Context>(frame: &mut Frame<C>, data: &DataArena, context: &mut C, size: Size<C::Scalar>) {
    let mut geometry = std::mem::take(&mut frame.node_geometry);
    let mut requested = frame.frame_requested;
    let mut cx = LayoutCx {
        frame,
        data,
        geometry: &mut geometry,
        context,
        frame_requested: &mut requested,
        node: NodeId::new(0),
        default_item: DataId::NONE,
        item: PhantomData::<fn() -> ()>,
    };
    cx.resolve(NodeId::new(0), Constraints::tight(size));
    for index in 1..cx.frame.nodes.len() {
        if !cx.frame.nodes[index].out_of_flow {
            continue;
        }
        let area = &mut cx.geometry[index].area;
        area.x = C::Scalar::ZERO;
        area.y = C::Scalar::ZERO;
        cx.resolve(
            NodeId::new(index),
            Constraints::loose(Size::uniform(C::Scalar::UNBOUNDED)),
        );
    }
    frame.node_geometry = geometry;
    frame.frame_requested = requested;
}

pub fn run<'a, C: Context, L: Layout<C>>(
    cx: &mut LayoutCx<'a, C>,
    id: DataId,
    bounds: Constraints<C::Scalar>,
) -> Size<C::Scalar> {
    let layout = cx.data.load::<L>(id);
    layout.layout(cx.cast::<L::Item>(), bounds)
}
