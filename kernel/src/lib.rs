mod geometry;

pub use geometry::{Constraints, Point, Rect, Size};

use std::{
    any::TypeId,
    marker::PhantomData,
    mem::{MaybeUninit, align_of, size_of},
};

pub trait Renderer {
    fn begin(&mut self, frame: FrameInfo);

    fn end(&mut self);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameInfo {
    pub size: Size,
}

pub trait Measure<R: Renderer> {
    type Output;

    fn measure(self, renderer: &mut R) -> Self::Output;
}

pub trait Paint<R: Renderer> {
    fn paint(self, renderer: &mut R);
}

pub trait ClipCommand<R: Renderer> {
    fn push(self, renderer: &mut R);

    fn pop(renderer: &mut R);
}

pub trait Leaf<R: Renderer>: Copy + 'static {
    fn measure(&self, cx: &mut MeasureCx<'_, R>, constraints: Constraints) -> Size;

    fn paint(&self, cx: &mut PaintCx<'_, R>, area: Rect);
}

pub struct MeasureCx<'a, R: Renderer> {
    renderer: &'a mut R,
}

impl<R: Renderer> MeasureCx<'_, R> {
    pub fn measure<M: Measure<R>>(&mut self, request: M) -> M::Output {
        request.measure(self.renderer)
    }
}

pub struct PaintCx<'a, R: Renderer> {
    renderer: &'a mut R,
}

impl<R: Renderer> PaintCx<'_, R> {
    pub fn paint<P: Paint<R>>(&mut self, paint: P) {
        paint.paint(self.renderer)
    }
}

pub trait Clip<R: Renderer>: Copy + 'static {
    fn bounds(&self, area: Rect) -> Rect;

    fn contains(&self, area: Rect, point: Point) -> bool;

    fn push(&self, cx: &mut ClipCx<'_, R>, area: Rect);

    fn pop(&self, cx: &mut ClipCx<'_, R>);
}

pub struct ClipCx<'a, R: Renderer> {
    renderer: &'a mut R,
}

impl<R: Renderer> ClipCx<'_, R> {
    pub fn push<C: ClipCommand<R>>(&mut self, clip: C) {
        clip.push(self.renderer)
    }

    pub fn pop<C: ClipCommand<R>>(&mut self) {
        C::pop(self.renderer)
    }
}

pub trait Layout<R: Renderer>: Copy + 'static {
    type Item: Copy + 'static;

    fn layout(&self, cx: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size;
}

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
        if self.next > self.end {
            return None;
        }
        let node = NodeId(self.next as u32);
        // safety: node storage is frozen while layout runs
        let stored = unsafe { &*self.nodes.add(self.next) };
        self.next = stored.subtree_end as usize + 1;
        Some(node)
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
    pub fn render(&mut self, renderer: &mut R, size: Size, build: impl FnOnce(&mut Ui<'_, R>)) {
        self.nodes.clear();
        self.data.words.clear();

        let mut ui = Ui {
            frame: self,
            parent: None,
        };
        build(&mut ui);
        assert!(ui.parent.is_none(), "layout scope was not closed");
        assert!(!ui.frame.nodes.is_empty(), "frame is empty");
        assert_eq!(
            ui.frame.nodes[0].subtree_end as usize,
            ui.frame.nodes.len() - 1,
            "a frame must have exactly one root"
        );

        ui.frame
            .layout_node(NodeId(0), renderer, Constraints::tight(size));
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
        paint(ui.frame, renderer, NodeId(0), Point::ZERO);
        renderer.end();
    }

    fn layout_node(&mut self, node: NodeId, renderer: &mut R, constraints: Constraints) -> Size {
        let stored = self.nodes[node.index()];
        let size = if let Some(layout) = stored.layout {
            let run = self.layout_kinds[layout.kind as usize].layout;
            run(self, node, renderer, layout.data, constraints)
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

    fn store_layout<L: Layout<R>>(&mut self, layout: L) -> StoredLayout {
        let type_id = TypeId::of::<L>();
        let kind = self
            .layout_kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                self.layout_kinds.push(LayoutKind {
                    type_id,
                    layout: run_layout::<R, L>,
                });
                self.layout_kinds.len() - 1
            });
        StoredLayout {
            kind: u16::try_from(kind).expect("too many layout types"),
            data: self.data.store(layout),
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

pub struct Ui<'a, R: Renderer> {
    frame: &'a mut Frame<R>,
    parent: Option<NodeId>,
}

impl<'frame, R: Renderer> Ui<'frame, R> {
    pub fn add<L: Leaf<R>>(&mut self, leaf: L) -> NodeId {
        let base = self.frame.store_leaf(leaf);
        self.frame.push_node(self.parent, Some(base), None)
    }

    pub fn layout<L: Layout<R>>(&mut self, layout: L) -> Container<'_, 'frame, R, L> {
        let layout = self.frame.store_layout(layout);
        let node = self.frame.push_node(self.parent, None, Some(layout));
        let parent = self.parent;
        self.parent = Some(node);
        Container {
            ui: self,
            node,
            parent,
            marker: PhantomData,
        }
    }

    pub fn layout_with<B: Leaf<R>, L: Layout<R>>(
        &mut self,
        base: B,
        layout: L,
    ) -> Container<'_, 'frame, R, L> {
        let base = self.frame.store_leaf(base);
        let layout = self.frame.store_layout(layout);
        let node = self.frame.push_node(self.parent, Some(base), Some(layout));
        let parent = self.parent;
        self.parent = Some(node);
        Container {
            ui: self,
            node,
            parent,
            marker: PhantomData,
        }
    }
}

pub struct Container<'ui, 'frame, R, L>
where
    R: Renderer,
    L: Layout<R>,
{
    ui: &'ui mut Ui<'frame, R>,
    node: NodeId,
    parent: Option<NodeId>,
    marker: PhantomData<L>,
}

impl<R: Renderer, L: Layout<R>> Container<'_, '_, R, L> {
    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn clip<C: Clip<R>>(self, clip: C) -> Self {
        assert!(
            self.ui.frame.nodes[self.node.index()].clip.is_none(),
            "layout already has a clip"
        );
        let clip = self.ui.frame.store_clip(clip);
        self.ui.frame.nodes[self.node.index()].clip = Some(clip);
        self
    }

    pub fn add<O>(&mut self, item: L::Item, child: impl FnOnce(&mut Ui<'_, R>) -> O) -> O {
        let start = self.ui.frame.nodes.len();
        let output = child(self.ui);
        let end = self.ui.frame.nodes.len();
        assert!(end > start, "layout child did not add a node");

        let child = NodeId(start as u32);
        assert_eq!(
            self.ui.frame.nodes[child.index()].parent,
            Some(self.node),
            "layout child was added outside its parent"
        );
        assert_eq!(
            self.ui.frame.nodes[child.index()].subtree_end as usize + 1,
            end,
            "a layout item must contain exactly one root"
        );
        let data = self.ui.frame.data.store(item);
        self.ui.frame.nodes[child.index()].item = Some(data);
        output
    }
}

impl<R: Renderer, L: Layout<R>> Drop for Container<'_, '_, R, L> {
    fn drop(&mut self) {
        assert_eq!(
            self.ui.parent,
            Some(self.node),
            "layout scopes closed out of order"
        );
        self.ui.frame.nodes[self.node.index()].subtree_end =
            u32::try_from(self.ui.frame.nodes.len() - 1).expect("too many frame nodes");
        self.ui.parent = self.parent;
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
    data.load::<L>(id)
        .measure(&mut MeasureCx { renderer }, constraints)
}

fn paint_leaf<R: Renderer, L: Leaf<R>>(data: &DataArena, id: DataId, renderer: &mut R, area: Rect) {
    data.load::<L>(id).paint(&mut PaintCx { renderer }, area)
}

fn run_layout<R: Renderer, L: Layout<R>>(
    frame: &mut Frame<R>,
    node: NodeId,
    renderer: &mut R,
    id: DataId,
    constraints: Constraints,
) -> Size {
    let layout = frame.data.load::<L>(id);
    let nodes = frame.nodes.as_ptr();
    layout.layout(
        &mut LayoutCx {
            frame,
            renderer,
            node,
            nodes,
            item: PhantomData,
        },
        constraints,
    )
}

fn push_clip<R: Renderer, C: Clip<R>>(data: &DataArena, id: DataId, renderer: &mut R, area: Rect) {
    data.load::<C>(id).push(&mut ClipCx { renderer }, area)
}

fn pop_clip<R: Renderer, C: Clip<R>>(data: &DataArena, id: DataId, renderer: &mut R) {
    data.load::<C>(id).pop(&mut ClipCx { renderer })
}

#[derive(Clone, Copy)]
#[repr(transparent)]
struct DataId(u32);

#[repr(C, align(8))]
struct Word(MaybeUninit<[u8; 8]>);

#[derive(Default)]
struct DataArena {
    words: Vec<Word>,
}

impl DataArena {
    fn store<T: Copy>(&mut self, value: T) -> DataId {
        const {
            assert!(align_of::<T>() <= 8, "frame data alignment exceeds 8 bytes");
        }
        let start = self.words.len();
        let words = size_of::<T>().div_ceil(8);
        self.words
            .resize_with(start + words, || Word(MaybeUninit::uninit()));
        // safety: each allocation begins at an 8-byte-aligned word
        unsafe { self.words.as_mut_ptr().add(start).cast::<T>().write(value) };
        DataId(u32::try_from(start).expect("too much frame data"))
    }

    fn load<T: Copy>(&self, id: DataId) -> T {
        let start = id.0 as usize;
        let words = size_of::<T>().div_ceil(8);
        assert!(start + words <= self.words.len());
        assert!(align_of::<T>() <= 8);
        // safety: the owning record uses the same type that was stored at id
        unsafe { self.words.as_ptr().add(start).cast::<T>().read() }
    }
}
