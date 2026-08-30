use std::{any::TypeId, time::Duration};

pub mod container;
pub mod interaction;
pub mod layout;
pub mod paint;
pub mod position;
pub mod transition;
pub mod ui;

pub use container::{Absolute, Anchor, Container, LayerId, PositionTarget};
pub use ui::Ui;

use crate::{
    animation::Transition,
    arena::{DataArena, DataId},
    clip::Clip,
    geometry::{Constraints, Point, Rect, Sides, Size},
    input::Input,
    interact::WidgetId,
    layout::Layout,
    leaf::Leaf,
    renderer::Renderer,
};

pub struct Frame<R: Renderer> {
    nodes: Vec<Node>,
    leaf_kinds: Vec<LeafKind<R>>,
    layout_kinds: Vec<LayoutKind<R>>,
    clip_kinds: Vec<ClipKind<R>>,
    data: DataArena,
    layers: Vec<Layer>,
    paint_order: Vec<NodeId>,
    order_stack: Vec<NodeId>,
    resolved_clips: Vec<ResolvedClip>,
    interaction: interaction::InteractionState,
    geometry_previous: Vec<(WidgetId, Rect)>,
    geometry_current: Vec<(WidgetId, Rect)>,
    transitions: Vec<transition::TransitionState>,
    input: Input,
    time: Duration,
    screen: Rect,
    needs_paint_order: bool,
    frame_requested: bool,
}

impl<R: Renderer> Default for Frame<R> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            leaf_kinds: Vec::new(),
            layout_kinds: Vec::new(),
            clip_kinds: Vec::new(),
            data: DataArena::default(),
            layers: Vec::new(),
            paint_order: Vec::new(),
            order_stack: Vec::new(),
            resolved_clips: Vec::new(),
            interaction: interaction::InteractionState::default(),
            geometry_previous: Vec::new(),
            geometry_current: Vec::new(),
            transitions: Vec::new(),
            input: Input::None,
            time: Duration::ZERO,
            screen: Rect::default(),
            needs_paint_order: false,
            frame_requested: true,
        }
    }
}

impl<R: Renderer> Frame<R> {
    pub fn render(&mut self, renderer: &mut R, size: Size, build: impl FnOnce(Ui<'_, R>)) {
        self.frame_requested = false;
        self.record(renderer, size, Duration::ZERO, Input::None, true, build);
    }

    pub fn render_inputs(
        &mut self,
        renderer: &mut R,
        size: Size,
        time: Duration,
        inputs: impl IntoIterator<Item = Input>,
        mut build: impl FnMut(Ui<'_, R>),
    ) {
        self.frame_requested = false;
        let mut inputs = inputs.into_iter();
        let Some(first) = inputs.next() else {
            self.record(renderer, size, time, Input::None, true, &mut build);
            return;
        };
        let mut input = first;
        loop {
            let next = inputs.next();
            self.record(renderer, size, time, input, next.is_none(), &mut build);
            let Some(next) = next else {
                break;
            };
            input = next;
        }
    }

    pub fn has_pending_redraw(&self) -> bool {
        self.frame_requested
            || self
                .transitions
                .iter()
                .any(transition::TransitionState::is_active)
    }

    pub fn request_frame(&mut self) {
        self.frame_requested = true;
    }

    pub fn geometry(&self, id: WidgetId) -> Option<Rect> {
        self.geometry_previous
            .iter()
            .find_map(|(candidate, area)| (*candidate == id).then_some(*area))
    }

    fn record(
        &mut self,
        renderer: &mut R,
        size: Size,
        time: Duration,
        input: Input,
        render: bool,
        build: impl FnOnce(Ui<'_, R>),
    ) {
        self.nodes.clear();
        self.data.clear();
        self.layers.clear();
        self.paint_order.clear();
        self.resolved_clips.clear();
        self.needs_paint_order = false;
        self.input = input;
        self.time = time;
        self.screen = Rect::new(0.0, 0.0, size.width, size.height);
        for state in &mut self.transitions {
            state.seen = false;
        }
        self.interaction.begin(&input);

        build(ui::new(self, None));
        assert!(!self.nodes.is_empty(), "frame is empty");
        assert_eq!(
            self.nodes[0].subtree_end as usize,
            self.nodes.len() - 1,
            "a frame must have exactly one root"
        );

        transition::resolve(self, renderer, size);
        position::resolve(self);
        paint::resolve_order(self);
        paint::resolve_clips(self, self.screen);
        interaction::resolve(self, renderer);
        std::mem::swap(&mut self.geometry_previous, &mut self.geometry_current);
        self.geometry_current.clear();
        self.transitions.retain(|state| state.seen);
        if render {
            paint::render(self, renderer, size);
        }
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
        let mut size = constraints.constrain(size);
        if let Some(width) = stored.transition_width {
            size.width = width.clamp(constraints.min.width, constraints.max.width);
        }
        if let Some(height) = stored.transition_height {
            size.height = height.clamp(constraints.min.height, constraints.max.height);
        }
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
            positioned: None,
            layer: None,
            z_index: 0,
            id: None,
            hit: Sides::all(0.0),
            transition: None,
            transition_width: None,
            transition_height: None,
            resolved_clip: None,
            clip_bounds: Rect::default(),
        });
        id
    }

    fn add_layer(&mut self, owner: NodeId) -> LayerId {
        let id = container::layer_id(self.layers.len());
        self.layers.push(Layer { owner });
        id
    }

    fn set_absolute(&mut self, node: NodeId, absolute: Absolute) {
        let parent = self.nodes[node.index()]
            .parent
            .expect("the root cannot be absolutely positioned");
        let target = match absolute.target {
            PositionTarget::Parent => parent,
            PositionTarget::Node(target) => {
                assert!(
                    target.index() < node.index(),
                    "absolute target must be declared first"
                );
                target
            }
            PositionTarget::Screen => NodeId(0),
        };
        self.nodes[node.index()].positioned = Some(Positioned {
            target,
            target_anchor: absolute.target_anchor,
            child_anchor: absolute.child_anchor,
            offset: absolute.offset,
        });
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
    positioned: Option<Positioned>,
    layer: Option<LayerId>,
    z_index: i16,
    id: Option<WidgetId>,
    hit: Sides,
    transition: Option<Transition>,
    transition_width: Option<f32>,
    transition_height: Option<f32>,
    resolved_clip: Option<usize>,
    clip_bounds: Rect,
}

#[derive(Clone, Copy)]
struct Positioned {
    target: NodeId,
    target_anchor: Anchor,
    child_anchor: Anchor,
    offset: Point,
}

#[derive(Clone, Copy)]
struct Layer {
    owner: NodeId,
}

#[derive(Clone, Copy)]
struct ResolvedClip {
    parent: Option<usize>,
    clip: StoredClip,
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
