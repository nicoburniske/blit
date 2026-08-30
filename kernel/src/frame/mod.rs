use std::any::TypeId;

pub mod container;
pub mod layout;
pub mod ui;

pub use container::Container;
pub use ui::Ui;

use crate::{
    arena::{DataArena, DataId},
    clip::Clip,
    geometry::{Constraints, Point, Rect, Size},
    layout::Layout,
    leaf::Leaf,
    renderer::{FrameInfo, Renderer},
};

pub struct Frame<R: Renderer> {
    nodes: Vec<Node>,
    leaf_kinds: Vec<LeafKind<R>>,
    layout_kinds: Vec<LayoutKind<R>>,
    clip_kinds: Vec<ClipKind<R>>,
    data: DataArena,
}

impl<R: Renderer> Default for Frame<R> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            leaf_kinds: Vec::new(),
            layout_kinds: Vec::new(),
            clip_kinds: Vec::new(),
            data: DataArena::default(),
        }
    }
}

impl<R: Renderer> Frame<R> {
    pub fn render(&mut self, renderer: &mut R, size: Size, build: impl FnOnce(Ui<'_, R>)) {
        self.nodes.clear();
        self.data.clear();

        build(ui::new(self, None));
        assert!(!self.nodes.is_empty(), "frame is empty");
        assert_eq!(
            self.nodes[0].subtree_end as usize,
            self.nodes.len() - 1,
            "a frame must have exactly one root"
        );

        self.layout_node(NodeId(0), renderer, Constraints::tight(size));
        renderer.begin(FrameInfo { size });
        fn paint<R: Renderer>(frame: &Frame<R>, renderer: &mut R, node: NodeId, origin: Point) {
            let stored = frame.nodes[node.index()];
            let area = Rect::new(
                origin.x + stored.area.x,
                origin.y + stored.area.y,
                stored.area.width,
                stored.area.height,
            );
            if let Some(clip) = stored.clip {
                let push = frame.clip_kinds[clip.kind as usize].push;
                push(&frame.data, clip.data, renderer, area);
            }
            if let Some(base) = stored.base {
                let paint = frame.leaf_kinds[base.kind as usize].paint;
                paint(&frame.data, base.data, renderer, area);
            }

            let mut next = node.index() + 1;
            while next <= stored.subtree_end as usize {
                let child = NodeId(next as u32);
                paint(frame, renderer, child, Point::new(area.x, area.y));
                next = frame.nodes[next].subtree_end as usize + 1;
            }
            if let Some(clip) = stored.clip {
                let pop = frame.clip_kinds[clip.kind as usize].pop;
                pop(&frame.data, clip.data, renderer);
            }
        }
        paint(self, renderer, NodeId(0), Point::ZERO);
        renderer.end();
    }

    fn layout_node(&mut self, node: NodeId, renderer: &mut R, constraints: Constraints) -> Size {
        let stored = self.nodes[node.index()];
        let size = if let Some(stored) = stored.layout {
            let run = self.layout_kinds[stored.kind as usize].layout;
            run(self, node, renderer, stored.data, constraints)
        } else {
            assert!(
                stored.base.is_some(),
                "node has neither a base nor a layout"
            );
            self.measure_base(node, renderer, constraints)
        };
        let size = constraints.constrain(size);
        self.nodes[node.index()].area.width = size.width;
        self.nodes[node.index()].area.height = size.height;
        size
    }

    fn measure_base(&mut self, node: NodeId, renderer: &mut R, constraints: Constraints) -> Size {
        let Some(base) = self.nodes[node.index()].base else {
            return Size::ZERO;
        };
        let measure = self.leaf_kinds[base.kind as usize].measure;
        measure(&self.data, base.data, renderer, constraints)
    }

    fn store_leaf<L: Leaf<R>>(&mut self, leaf: L) -> StoredLeaf {
        let type_id = TypeId::of::<L>();
        let kind = self
            .leaf_kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                self.leaf_kinds.push(LeafKind {
                    type_id,
                    measure: measure_leaf::<R, L>,
                    paint: paint_leaf::<R, L>,
                });
                self.leaf_kinds.len() - 1
            });
        StoredLeaf {
            kind: u16::try_from(kind).expect("too many leaf types"),
            data: self.data.store(leaf),
        }
    }

    fn store_layout<L: Layout<R>>(&mut self, value: L) -> StoredLayout {
        let type_id = TypeId::of::<L>();
        let kind = self
            .layout_kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                self.layout_kinds.push(LayoutKind {
                    type_id,
                    layout: layout::run::<R, L>,
                });
                self.layout_kinds.len() - 1
            });
        StoredLayout {
            kind: u16::try_from(kind).expect("too many layout types"),
            data: self.data.store(value),
        }
    }

    fn store_clip<C: Clip<R>>(&mut self, clip: C) -> StoredClip {
        let type_id = TypeId::of::<C>();
        let kind = self
            .clip_kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                self.clip_kinds.push(ClipKind {
                    type_id,
                    push: push_clip::<R, C>,
                    pop: pop_clip::<R, C>,
                });
                self.clip_kinds.len() - 1
            });
        StoredClip {
            kind: u16::try_from(kind).expect("too many clip types"),
            data: self.data.store(clip),
        }
    }

    fn push_node(
        &mut self,
        parent: Option<NodeId>,
        base: Option<StoredLeaf>,
        layout: Option<StoredLayout>,
    ) -> NodeId {
        let id = NodeId(u32::try_from(self.nodes.len()).expect("too many frame nodes"));
        self.nodes.push(Node {
            parent,
            subtree_end: id.0,
            base,
            layout,
            clip: None,
            item: None,
            area: Rect::default(),
        });
        id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct NodeId(u32);

impl NodeId {
    fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy)]
struct Node {
    parent: Option<NodeId>,
    subtree_end: u32,
    base: Option<StoredLeaf>,
    layout: Option<StoredLayout>,
    clip: Option<StoredClip>,
    item: Option<DataId>,
    area: Rect,
}

#[derive(Clone, Copy)]
struct StoredLeaf {
    kind: u16,
    data: DataId,
}

#[derive(Clone, Copy)]
struct StoredLayout {
    kind: u16,
    data: DataId,
}

#[derive(Clone, Copy)]
struct StoredClip {
    kind: u16,
    data: DataId,
}

struct LeafKind<R: Renderer> {
    type_id: TypeId,
    measure: fn(&DataArena, DataId, &mut R, Constraints) -> Size,
    paint: fn(&DataArena, DataId, &mut R, Rect),
}

struct LayoutKind<R: Renderer> {
    type_id: TypeId,
    layout: fn(&mut Frame<R>, NodeId, &mut R, DataId, Constraints) -> Size,
}

struct ClipKind<R: Renderer> {
    type_id: TypeId,
    push: fn(&DataArena, DataId, &mut R, Rect),
    pop: fn(&DataArena, DataId, &mut R),
}

fn measure_leaf<R: Renderer, L: Leaf<R>>(
    data: &DataArena,
    id: DataId,
    renderer: &mut R,
    constraints: Constraints,
) -> Size {
    data.load::<L>(id).measure(renderer, constraints)
}

fn paint_leaf<R: Renderer, L: Leaf<R>>(data: &DataArena, id: DataId, renderer: &mut R, area: Rect) {
    data.load::<L>(id).paint(renderer, area)
}

fn push_clip<R: Renderer, C: Clip<R>>(data: &DataArena, id: DataId, renderer: &mut R, area: Rect) {
    data.load::<C>(id).push(renderer, area)
}

fn pop_clip<R: Renderer, C: Clip<R>>(data: &DataArena, id: DataId, renderer: &mut R) {
    data.load::<C>(id).pop(renderer)
}
