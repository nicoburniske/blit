use std::{any::TypeId, mem::size_of, time::Duration};

pub mod container;
pub mod interaction;
pub mod layout;
pub mod paint;
pub mod position;
pub mod transition;
pub mod ui;

pub use container::{Absolute, Anchor, Container, LayerId, PositionTarget, Sizing, Slot};
pub use ui::Ui;

/// retained memory used by a frame after its buffers have grown
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameMemory {
    pub node_size: usize,
    pub node_capacity: usize,
    pub heap_bytes: usize,
}

use crate::{
    Clip, FrameInfo, Leaf, Platform,
    animation::Transition,
    arena::{DataArena, DataId},
    geometry::{Constraints, Point, Rect, Sides, Size},
    input::Input,
    interact::WidgetId,
    layout::{Axis, Layout, LayoutResolution},
};

pub struct Frame<R: Platform> {
    nodes: Vec<Node>,
    leaves: Vec<StoredLeaf>,
    layouts: Vec<StoredLayout>,
    clips: Vec<StoredClip>,
    positioned: Vec<Positioned>,
    geometry: Vec<GeometryRecord>,
    leaf_kinds: Vec<LeafKind<R>>,
    layout_kinds: Vec<LayoutKind<R>>,
    clip_kinds: Vec<ClipKind<R>>,
    data: DataArena,
    layers: Vec<Layer>,
    paint_order: Vec<NodeId>,
    order_stack: Vec<NodeId>,
    resolved_clips: Vec<ResolvedClip>,
    active_clips: Vec<ResolvedClipId>,
    interaction: interaction::InteractionState,
    geometry_previous: Vec<(WidgetId, Rect)>,
    geometry_current: Vec<(WidgetId, Rect)>,
    animations: Vec<crate::animation::AnimationState>,
    transitions: Vec<transition::TransitionState>,
    timers: Vec<crate::timer::TimerState>,
    input: Input,
    time: Duration,
    screen: Rect,
    layout_resolution: LayoutResolution,
    needs_paint_order: bool,
    frame_requested: bool,
}

impl<R: Platform> Default for Frame<R> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            leaves: Vec::new(),
            layouts: Vec::new(),
            clips: Vec::new(),
            positioned: Vec::new(),
            geometry: Vec::new(),
            leaf_kinds: Vec::new(),
            layout_kinds: Vec::new(),
            clip_kinds: Vec::new(),
            data: DataArena::default(),
            layers: Vec::new(),
            paint_order: Vec::new(),
            order_stack: Vec::new(),
            resolved_clips: Vec::new(),
            active_clips: Vec::new(),
            interaction: interaction::InteractionState::default(),
            geometry_previous: Vec::new(),
            geometry_current: Vec::new(),
            animations: Vec::new(),
            transitions: Vec::new(),
            timers: Vec::new(),
            input: Input::None,
            time: Duration::ZERO,
            screen: Rect::default(),
            layout_resolution: LayoutResolution::Continuous,
            needs_paint_order: false,
            frame_requested: true,
        }
    }
}

impl<R: Platform> Frame<R> {
    pub fn render(&mut self, platform: &mut R, frame: FrameInfo, build: impl FnOnce(Ui<'_, R>)) {
        self.frame_requested = false;
        self.record(platform, frame, Duration::ZERO, Input::None, true, build);
    }

    pub fn render_inputs(
        &mut self,
        platform: &mut R,
        frame: FrameInfo,
        time: Duration,
        inputs: impl IntoIterator<Item = Input>,
        mut build: impl FnMut(Ui<'_, R>),
    ) {
        self.frame_requested = false;
        let mut inputs = inputs.into_iter();
        let Some(first) = inputs.next() else {
            self.record(platform, frame, time, Input::None, true, &mut build);
            return;
        };
        let mut input = first;
        loop {
            let next = inputs.next();
            self.record(platform, frame, time, input, next.is_none(), &mut build);
            let Some(next) = next else {
                break;
            };
            input = next;
        }
    }

    pub fn has_pending_redraw(&self) -> bool {
        self.frame_requested
            || self
                .animations
                .iter()
                .any(crate::animation::AnimationState::is_active)
            || self
                .transitions
                .iter()
                .any(transition::TransitionState::is_active)
    }

    pub fn next_timer_deadline(&self) -> Option<Duration> {
        self.timers
            .iter()
            .filter_map(crate::timer::TimerState::deadline)
            .min()
    }

    pub fn request_frame(&mut self) {
        self.frame_requested = true;
    }

    pub fn geometry(&self, id: WidgetId) -> Option<Rect> {
        self.geometry_previous
            .iter()
            .find_map(|(candidate, area)| (*candidate == id).then_some(*area))
    }

    pub fn memory(&self) -> FrameMemory {
        FrameMemory {
            node_size: size_of::<Node>(),
            node_capacity: self.nodes.capacity(),
            heap_bytes: self.nodes.capacity() * size_of::<Node>()
                + self.leaves.capacity() * size_of::<StoredLeaf>()
                + self.layouts.capacity() * size_of::<StoredLayout>()
                + self.clips.capacity() * size_of::<StoredClip>()
                + self.positioned.capacity() * size_of::<Positioned>()
                + self.geometry.capacity() * size_of::<GeometryRecord>()
                + self.leaf_kinds.capacity() * size_of::<LeafKind<R>>()
                + self.layout_kinds.capacity() * size_of::<LayoutKind<R>>()
                + self.clip_kinds.capacity() * size_of::<ClipKind<R>>()
                + self.data.heap_bytes()
                + self.layers.capacity() * size_of::<Layer>()
                + self.paint_order.capacity() * size_of::<NodeId>()
                + self.order_stack.capacity() * size_of::<NodeId>()
                + self.resolved_clips.capacity() * size_of::<ResolvedClip>()
                + self.active_clips.capacity() * size_of::<ResolvedClipId>()
                + self.geometry_previous.capacity() * size_of::<(WidgetId, Rect)>()
                + self.geometry_current.capacity() * size_of::<(WidgetId, Rect)>()
                + self.animations.capacity() * size_of::<crate::animation::AnimationState>()
                + self.transitions.capacity() * size_of::<transition::TransitionState>()
                + self.timers.capacity() * size_of::<crate::timer::TimerState>(),
        }
    }

    fn record(
        &mut self,
        platform: &mut R,
        frame: FrameInfo,
        time: Duration,
        input: Input,
        render: bool,
        build: impl FnOnce(Ui<'_, R>),
    ) {
        #[cfg(debug_assertions)]
        generation::begin();
        self.nodes.clear();
        self.leaves.clear();
        self.layouts.clear();
        self.clips.clear();
        self.positioned.clear();
        self.geometry.clear();
        self.data.clear();
        self.layers.clear();
        self.paint_order.clear();
        self.resolved_clips.clear();
        self.active_clips.clear();
        self.needs_paint_order = false;
        self.input = input;
        self.time = time;
        self.screen = Rect::new(0.0, 0.0, frame.size.width, frame.size.height);
        self.layout_resolution = frame.layout_resolution;
        for animation in &mut self.animations {
            animation.seen = false;
        }
        for state in &mut self.transitions {
            state.seen = false;
        }
        for timer in &mut self.timers {
            timer.seen = false;
        }
        self.interaction.begin(&input);

        build(ui::new(self, platform, None));
        assert!(!self.nodes.is_empty(), "frame is empty");
        assert_eq!(
            self.nodes[0].subtree_end as usize,
            self.nodes.len() - 1,
            "a frame must have exactly one root"
        );

        transition::resolve(self, platform, frame.size);
        position::resolve(self);
        paint::resolve_order(self);
        paint::resolve_clips(self);
        interaction::resolve(self, platform);
        std::mem::swap(&mut self.geometry_previous, &mut self.geometry_current);
        self.geometry_current.clear();
        self.animations.retain(|animation| animation.seen);
        self.transitions.retain(|state| state.seen);
        self.timers.retain(|timer| timer.seen);
        if render {
            paint::render(self, platform, frame);
        }
    }

    fn layout_node(&mut self, node: NodeId, platform: &mut R, constraints: Constraints) -> Size {
        let index = node.index();
        let size = if let Some(layout) = self.nodes[index].layout.index() {
            let stored = self.layouts[layout];
            let run = self.layout_kinds[stored.kind as usize].layout;
            run(self, node, platform, stored.data, constraints)
        } else {
            assert!(
                self.nodes[index].base.index().is_some(),
                "node has neither a base nor a layout"
            );
            self.measure_base(node, platform, constraints)
        };
        let size = constraints.constrain(size);
        self.nodes[index].area.width = size.width;
        self.nodes[index].area.height = size.height;
        size
    }

    fn measure_base(&mut self, node: NodeId, platform: &mut R, constraints: Constraints) -> Size {
        let Some(base) = self.nodes[node.index()].base.index() else {
            return Size::ZERO;
        };
        let base = self.leaves[base];
        let measure = self.leaf_kinds[base.kind as usize].measure;
        measure(&self.data, base.data, platform, constraints)
    }

    fn store_leaf<L: Leaf<R>>(&mut self, leaf: L) -> StoredLeafId {
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
        let id = StoredLeafId::new(self.leaves.len());
        self.leaves.push(StoredLeaf {
            kind: u16::try_from(kind).expect("too many leaf types"),
            data: self.data.store(leaf),
        });
        id
    }

    fn store_layout<L: Layout<R>>(&mut self, value: L) -> StoredLayoutId {
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
        let id = StoredLayoutId::new(self.layouts.len());
        self.layouts.push(StoredLayout {
            kind: u16::try_from(kind).expect("too many layout types"),
            data: self.data.store(value),
            offset: Point::ZERO,
        });
        id
    }

    fn store_clip<C: Clip<R>>(&mut self, clip: C) -> StoredClipId {
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
        let id = StoredClipId::new(self.clips.len());
        self.clips.push(StoredClip {
            kind: u16::try_from(kind).expect("too many clip types"),
            data: self.data.store(clip),
        });
        id
    }

    fn push_node(
        &mut self,
        parent: Option<NodeId>,
        base: Option<StoredLeafId>,
        layout: Option<StoredLayoutId>,
    ) -> NodeId {
        let id = self.node_id(self.nodes.len());
        self.nodes.push(Node {
            parent: parent.unwrap_or(id),
            subtree_end: id.value,
            base: base.unwrap_or(StoredLeafId::NONE),
            layout: layout.unwrap_or(StoredLayoutId::NONE),
            clip: StoredClipId::NONE,
            item: DataId::NONE,
            area: Rect::default(),
            positioned: PositionedId::NONE,
            slot: Slot::new(),
            geometry: GeometryId::NONE,
            resolved_clip: ResolvedClipId::NONE,
        });
        id
    }

    fn add_layer(&mut self, owner: NodeId) -> LayerId {
        let id = container::layer_id(self.layers.len());
        self.layers.push(Layer { owner });
        id
    }

    fn set_absolute(&mut self, node: NodeId, absolute: Absolute) {
        let parent = self.nodes[node.index()].parent;
        assert_ne!(parent, node, "the root cannot be absolutely positioned");
        let target = match absolute.target {
            PositionTarget::Parent => parent,
            PositionTarget::Node(target) => {
                assert!(
                    target.index() < node.index(),
                    "absolute target must be declared first"
                );
                target
            }
            PositionTarget::Screen => self.node_id(0),
        };
        let positioned = PositionedId::new(self.positioned.len());
        self.positioned.push(Positioned {
            target,
            uses_target_content_origin: matches!(absolute.target, PositionTarget::Parent),
            target_anchor: absolute.target_anchor,
            child_anchor: absolute.child_anchor,
            offset: absolute.offset,
        });
        self.nodes[node.index()].positioned = positioned;
    }

    fn geometry_mut(&mut self, node: NodeId) -> &mut GeometryRecord {
        let index = if let Some(index) = self.nodes[node.index()].geometry.index() {
            index
        } else {
            let id = GeometryId::new(self.geometry.len());
            self.nodes[node.index()].geometry = id;
            self.geometry.push(GeometryRecord {
                node,
                id: None,
                hit: Sides::all(0.0),
                transition: None,
            });
            id.index().unwrap()
        };
        &mut self.geometry[index]
    }

    fn set_id(&mut self, node: NodeId, id: WidgetId) {
        self.geometry_mut(node).id = Some(id);
    }

    fn set_hit(&mut self, node: NodeId, hit: Sides) {
        self.geometry_mut(node).hit = hit;
    }

    fn set_transition(&mut self, node: NodeId, transition: Transition) {
        self.geometry_mut(node).transition = Some(transition);
    }

    fn layout_offset(&self, node: NodeId) -> Point {
        self.nodes[node.index()]
            .layout
            .index()
            .map_or(Point::ZERO, |layout| self.layouts[layout].offset)
    }

    fn clip_bounds(&self, clip: ResolvedClipId) -> Rect {
        clip.index()
            .map_or(self.screen, |clip| self.resolved_clips[clip].bounds)
    }

    fn set_slot(&mut self, node: NodeId, mut slot: Slot) {
        slot.width = self.layout_resolution.sizing(Axis::Horizontal, slot.width);
        slot.height = self.layout_resolution.sizing(Axis::Vertical, slot.height);
        if let Some(layer) = slot.layer {
            let layer = container::layer_index(layer);
            assert!(
                layer < self.layers.len(),
                "layer does not belong to this frame"
            );
            assert!(
                self.layers[layer].owner.index() < node.index(),
                "a layer can only contain nodes declared after its owner"
            );
        }
        self.needs_paint_order |= slot.z_index != 0 || slot.layer.is_some();
        self.nodes[node.index()].slot = slot;
    }

    fn node_id(&self, index: usize) -> NodeId {
        NodeId {
            value: u32::try_from(index).expect("too many frame nodes"),
            #[cfg(debug_assertions)]
            generation: generation::get(),
        }
    }
}

/// identifies a node only during the current render
///
/// do not store this across renders
#[cfg_attr(not(debug_assertions), repr(transparent))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeId {
    value: u32,
    #[cfg(debug_assertions)]
    generation: u16,
}

impl NodeId {
    fn index(self) -> usize {
        #[cfg(debug_assertions)]
        generation::assert(self.generation);
        self.value as usize
    }
}

#[cfg(debug_assertions)]
mod generation {
    use std::{
        cell::Cell,
        sync::atomic::{AtomicU16, Ordering},
    };

    static NEXT: AtomicU16 = AtomicU16::new(1);

    thread_local! {
        static CURRENT: Cell<u16> = const { Cell::new(0) };
    }

    pub fn begin() {
        CURRENT.set(NEXT.fetch_add(1, Ordering::Relaxed));
    }

    pub fn get() -> u16 {
        CURRENT.get()
    }

    #[inline]
    pub fn assert(id: u16) {
        assert_eq!(id, get(), "id belongs to another frame");
    }
}

#[derive(Clone, Copy)]
struct Node {
    parent: NodeId,
    subtree_end: u32,
    base: StoredLeafId,
    layout: StoredLayoutId,
    clip: StoredClipId,
    item: DataId,
    area: Rect,
    positioned: PositionedId,
    slot: Slot,
    geometry: GeometryId,
    resolved_clip: ResolvedClipId,
}

#[derive(Clone, Copy)]
struct Positioned {
    target: NodeId,
    uses_target_content_origin: bool,
    target_anchor: Anchor,
    child_anchor: Anchor,
    offset: Point,
}

#[derive(Clone, Copy)]
struct GeometryRecord {
    node: NodeId,
    id: Option<WidgetId>,
    hit: Sides,
    transition: Option<Transition>,
}

#[derive(Clone, Copy)]
struct Layer {
    owner: NodeId,
}

#[derive(Clone, Copy)]
struct ResolvedClip {
    parent: ResolvedClipId,
    depth: u32,
    clip: StoredClipId,
    area: Rect,
    bounds: Rect,
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
    offset: Point,
}

#[derive(Clone, Copy)]
struct StoredClip {
    kind: u16,
    data: DataId,
}

#[derive(Clone, Copy)]
struct Index<T>(u32, std::marker::PhantomData<fn() -> T>);

impl<T> Index<T> {
    const NONE: Self = Self(u32::MAX, std::marker::PhantomData);

    fn new(index: usize) -> Self {
        Self(
            u32::try_from(index).expect("too many frame values"),
            std::marker::PhantomData,
        )
    }

    fn index(self) -> Option<usize> {
        (self.0 != u32::MAX).then_some(self.0 as usize)
    }
}

type StoredLeafId = Index<StoredLeaf>;
type StoredLayoutId = Index<StoredLayout>;
type StoredClipId = Index<StoredClip>;
type PositionedId = Index<Positioned>;
type GeometryId = Index<GeometryRecord>;
type ResolvedClipId = Index<ResolvedClip>;

struct LeafKind<R: Platform> {
    type_id: TypeId,
    measure: fn(&DataArena, DataId, &mut R, Constraints) -> Size,
    paint: fn(&DataArena, DataId, &mut R, Rect),
}

struct LayoutKind<R: Platform> {
    type_id: TypeId,
    layout: fn(&mut Frame<R>, NodeId, &mut R, DataId, Constraints) -> Size,
}

struct ClipKind<R: Platform> {
    type_id: TypeId,
    push: fn(&DataArena, DataId, &mut R, Rect),
    pop: fn(&DataArena, DataId, &mut R),
}

fn measure_leaf<R: Platform, L: Leaf<R>>(
    data: &DataArena,
    id: DataId,
    platform: &mut R,
    constraints: Constraints,
) -> Size {
    data.load::<L>(id).measure(platform, constraints)
}

fn paint_leaf<R: Platform, L: Leaf<R>>(data: &DataArena, id: DataId, platform: &mut R, area: Rect) {
    data.load::<L>(id).paint(platform, area)
}

fn push_clip<R: Platform, C: Clip<R>>(data: &DataArena, id: DataId, platform: &mut R, area: Rect) {
    data.load::<C>(id).push(platform, area)
}

fn pop_clip<R: Platform, C: Clip<R>>(data: &DataArena, id: DataId, platform: &mut R) {
    data.load::<C>(id).pop(platform)
}
