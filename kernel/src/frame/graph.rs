pub struct Frame<C> {
    nodes: Vec<StoredNode>,
    current_parent: Option<NodeId>,
    atoms: Vec<StoredAtom>,
    layouts: Vec<StoredLayout>,
    clips: Vec<StoredClip>,
    positioned: Vec<Positioned>,
    geometry: Vec<GeometryRecord>,
    atom_kinds: Vec<AtomKind<C>>,
    layout_kinds: Vec<LayoutKind<C>>,
    clip_kinds: Vec<ClipKind<C>>,
    data: DataArena,
    named_nodes: HashMap<WidgetId, Option<NodeId>, BuildHasherDefault<WidgetIdHasher>>,
    paint_links: Vec<PaintLinks>,
    paint_order: Vec<NodeId>,
    order_stack: Vec<NodeId>,
    resolved_clips: Vec<ResolvedClip>,
    active_clips: Vec<ResolvedClipId>,
    interaction: interaction::InteractionState,
    geometry_previous: Vec<(WidgetId, Rect)>,
    geometry_current: Vec<(WidgetId, Rect)>,
    animations: Vec<animation::AnimationState>,
    transitions: Vec<transition::TransitionState>,
    target_sizes: Vec<Size>,
    timers: Vec<timer::TimerState>,
    input: Input,
    time: Duration,
    screen: Rect,
    layout_resolution: LayoutResolution,
    resized: bool,
    frame_requested: bool,
}

impl<C> Default for Frame<C> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            current_parent: None,
            atoms: Vec::new(),
            layouts: Vec::new(),
            clips: Vec::new(),
            positioned: Vec::new(),
            geometry: Vec::new(),
            atom_kinds: Vec::new(),
            layout_kinds: Vec::new(),
            clip_kinds: Vec::new(),
            data: DataArena::default(),
            named_nodes: HashMap::default(),
            paint_links: Vec::new(),
            paint_order: Vec::new(),
            order_stack: Vec::new(),
            resolved_clips: Vec::new(),
            active_clips: Vec::new(),
            interaction: interaction::InteractionState::default(),
            geometry_previous: Vec::new(),
            geometry_current: Vec::new(),
            animations: Vec::new(),
            transitions: Vec::new(),
            target_sizes: Vec::new(),
            timers: Vec::new(),
            input: Input::None,
            time: Duration::ZERO,
            screen: Rect::default(),
            layout_resolution: LayoutResolution::Continuous,
            resized: false,
            frame_requested: true,
        }
    }
}

impl<C> Frame<C> {
    /// rebuilds the frame graph for one input
    pub fn build<W: Widget<C>>(
        &mut self,
        context: &mut C,
        frame: FrameInfo,
        time: Duration,
        input: Input,
        widget: W,
    ) -> W::Response {
        self.frame_requested = false;
        self.record(context, frame, time, input, widget)
    }

    /// resolves layout, positioning, clipping and interaction for the built graph
    pub fn layout(&mut self, context: &mut C) {
        let mut data = std::mem::take(&mut self.data);
        transition::resolve(self, &mut data, context, self.screen.size(), self.resized);
        position::resolve(self);
        paint::resolve_order(self);
        paint::resolve_clips(self);
        interaction::resolve(self);
        std::mem::swap(&mut self.geometry_previous, &mut self.geometry_current);
        self.geometry_current.clear();
        self.animations.retain(|animation| animation.seen);
        self.transitions.retain(|state| state.seen);
        self.timers.retain(|timer| timer.seen);
        self.named_nodes.retain(|_, node| node.is_some());
        self.data = data;
    }

    /// paints the resolved graph and releases its retained values
    pub fn paint(&mut self, context: &mut C) {
        let mut data = std::mem::take(&mut self.data);
        paint::render(self, &data, context);
        data.clear();
        self.data = data;
    }

    pub fn has_pending_redraw(&self) -> bool {
        self.frame_requested
            || self
                .animations
                .iter()
                .any(animation::AnimationState::is_active)
            || self
                .transitions
                .iter()
                .any(transition::TransitionState::is_active)
    }

    pub fn next_timer_deadline(&self) -> Option<Duration> {
        self.timers
            .iter()
            .filter_map(timer::TimerState::deadline)
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

    fn record<W: Widget<C>>(
        &mut self,
        context: &mut C,
        frame: FrameInfo,
        time: Duration,
        input: Input,
        widget: W,
    ) -> W::Response {
        #[cfg(debug_assertions)]
        generation::begin();
        self.nodes.clear();
        self.current_parent = None;
        self.atoms.clear();
        self.layouts.clear();
        self.clips.clear();
        self.positioned.clear();
        self.geometry.clear();
        self.data.clear();
        // retain names but require fresh bindings for each build
        for node in self.named_nodes.values_mut() {
            *node = None;
        }
        self.paint_order.clear();
        self.resolved_clips.clear();
        self.active_clips.clear();
        self.input = input;
        self.time = time;
        self.resized = self.screen.size() != frame.size;
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

        let output = {
            let root = self.push_node();
            self.current_parent = Some(root);
            widget.build(Ui::new(&mut *self, &mut *context, root))
        };
        assert_eq!(
            self.nodes[0].subtree_end as usize,
            self.nodes.len() - 1,
            "a frame must have exactly one root"
        );

        output
    }

    fn layout_node(
        &mut self,
        data: &DataArena,
        node: NodeId,
        context: &mut C,
        constraints: Constraints,
    ) -> Size {
        let index = node.index();
        let size = if let Some(layout) = self.nodes[index].layout.index() {
            let stored = self.layouts[layout];
            let run = self.layout_kinds[stored.kind as usize].layout;
            run(data, self, node, context, stored.data, constraints)
        } else if constraints.min == constraints.max {
            constraints.min
        } else {
            constraints.constrain(self.measure_base(data, node, context, constraints))
        };
        self.nodes[index].area.width = size.width;
        self.nodes[index].area.height = size.height;
        size
    }

    fn measure_base(
        &mut self,
        data: &DataArena,
        node: NodeId,
        context: &mut C,
        constraints: Constraints,
    ) -> Size {
        let mut size = Size::ZERO;
        let mut atom = self.nodes[node.index()].first_atom;
        while let Some(index) = atom.index() {
            let stored = self.atoms[index];
            let measure = self.atom_kinds[stored.kind as usize].measure;
            let measured = measure(data, stored.data, context, constraints);
            size = size.max(measured);
            atom = stored.next;
        }
        size
    }

    fn push_atom<A: Atom<C>>(&mut self, node: NodeId, atom: A) {
        let type_id = TypeId::of::<A>();
        let kind = self
            .atom_kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                self.atom_kinds.push(AtomKind {
                    type_id,
                    measure: measure_atom::<C, A>,
                    paint_bounds: paint_bounds_atom::<C, A>,
                    paint: paint_atom::<C, A>,
                });
                self.atom_kinds.len() - 1
            });
        let id = StoredAtomId::new(self.atoms.len());
        self.atoms.push(StoredAtom {
            kind: u16::try_from(kind).expect("too many atom types"),
            data: self.data.store(atom),
            next: StoredAtomId::NONE,
        });
        let node = node.index();
        if let Some(last) = self.nodes[node].last_atom.index() {
            self.atoms[last].next = id;
        } else {
            self.nodes[node].first_atom = id;
        }
        self.nodes[node].last_atom = id;
    }

    fn store_layout<L: Layout<C>>(&mut self, value: L) -> StoredLayoutId {
        let type_id = TypeId::of::<L>();
        let kind = self
            .layout_kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                self.layout_kinds.push(LayoutKind {
                    type_id,
                    layout: layout::run::<C, L>,
                    override_size: layout::override_item::<C, L>,
                    store_default: layout::store_default::<L::Item>,
                });
                self.layout_kinds.len() - 1
            });
        let id = StoredLayoutId::new(self.layouts.len());
        self.layouts.push(StoredLayout {
            kind: u16::try_from(kind).expect("too many layout kinds"),
            data: self.data.store(value),
            default_item: DataId::NONE,
            offset: Point::ZERO,
        });
        id
    }

    fn store_clip<X: Clip<C>>(&mut self, clip: X) -> StoredClipId {
        let type_id = TypeId::of::<X>();
        let kind = self
            .clip_kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                self.clip_kinds.push(ClipKind {
                    type_id,
                    push: push_clip::<C, X>,
                    pop: pop_clip::<C, X>,
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

    fn push_node(&mut self) -> NodeId {
        let id = self.node_id(self.nodes.len());
        self.nodes.push(StoredNode {
            widget_id: None,
            parent: self.current_parent.unwrap_or(id),
            visual_parent: self.current_parent.unwrap_or(id),
            subtree_end: id.value,
            first_atom: StoredAtomId::NONE,
            last_atom: StoredAtomId::NONE,
            layout: StoredLayoutId::NONE,
            clip: StoredClipId::NONE,
            item: DataId::NONE,
            area: Rect::default(),
            positioned: PositionedId::NONE,
            z_index: 0,
            geometry: GeometryId::NONE,
            resolved_clip: ResolvedClipId::NONE,
            #[cfg(debug_assertions)]
            layout_state: LayoutState::Unlaid,
        });
        id
    }

    fn push_child<I: Default + 'static>(&mut self) -> NodeId {
        let node = self.push_node();
        let parent = self.nodes[node.index()].parent;
        let layout = self.nodes[parent.index()].layout.index().unwrap();
        if self.layouts[layout].default_item.offset().is_none() {
            self.layouts[layout].default_item = self.data.store(I::default());
        }
        self.current_parent = Some(node);
        node
    }

    fn set_absolute(&mut self, node: NodeId, absolute: Absolute) {
        let target = self.resolve_target(node, absolute.target);
        let positioned = PositionedId::new(self.positioned.len());
        self.nodes[node.index()].item = self.data.store(AbsoluteSizing {
            width: self
                .layout_resolution
                .sizing(Axis::Horizontal, absolute.width),
            height: self
                .layout_resolution
                .sizing(Axis::Vertical, absolute.height),
        });
        self.positioned.push(Positioned {
            target,
            uses_target_content_origin: matches!(absolute.target, NodeTarget::Parent),
            target_anchor: absolute.target_anchor,
            child_anchor: absolute.child_anchor,
            offset: absolute.offset,
        });
        self.nodes[node.index()].positioned = positioned;
    }

    fn resolve_target(&self, node: NodeId, target: NodeTarget) -> NodeId {
        let target = match target {
            NodeTarget::Parent => self.nodes[node.index()].parent,
            NodeTarget::Root => self.node_id(0),
            NodeTarget::Node(id) => id,
            NodeTarget::Widget(id) => self
                .named_nodes
                .get(&id)
                .copied()
                .flatten()
                .expect("target widget id must already be assigned"),
        };
        assert!(
            target.index() < node.index(),
            "target must be declared before its node"
        );
        target
    }

    fn geometry_mut(&mut self, node: NodeId) -> &mut GeometryRecord {
        let index = if let Some(index) = self.nodes[node.index()].geometry.index() {
            index
        } else {
            let id = GeometryId::new(self.geometry.len());
            self.nodes[node.index()].geometry = id;
            self.geometry.push(GeometryRecord {
                node,
                hit: Sides::all(0.0),
                transition: None,
                transition_size: Size::ZERO,
                transition_properties: crate::TransitionProperties::NONE,
            });
            id.index().unwrap()
        };
        &mut self.geometry[index]
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

    fn node_id(&self, index: usize) -> NodeId {
        NodeId::new(index)
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
struct StoredNode {
    widget_id: Option<WidgetId>,
    parent: NodeId,
    visual_parent: NodeId,
    subtree_end: u32,
    first_atom: StoredAtomId,
    last_atom: StoredAtomId,
    layout: StoredLayoutId,
    clip: StoredClipId,
    item: DataId,
    area: Rect,
    positioned: PositionedId,
    z_index: i16,
    geometry: GeometryId,
    resolved_clip: ResolvedClipId,
    #[cfg(debug_assertions)]
    layout_state: LayoutState,
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayoutState {
    Unlaid,
    Laid,
    Positioned,
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
struct AbsoluteSizing {
    width: Sizing,
    height: Sizing,
}

#[derive(Clone, Copy)]
struct GeometryRecord {
    node: NodeId,
    hit: Sides,
    transition: Option<Transition>,
    transition_size: Size,
    transition_properties: crate::TransitionProperties,
}

#[derive(Clone, Copy, Default)]
struct PaintLinks {
    first_child: u32,
    next_sibling: u32,
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
struct StoredAtom {
    kind: u16,
    data: DataId,
    next: StoredAtomId,
}

#[derive(Clone, Copy)]
struct StoredLayout {
    kind: u16,
    data: DataId,
    default_item: DataId,
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

type StoredAtomId = Index<StoredAtom>;
type StoredLayoutId = Index<StoredLayout>;
type StoredClipId = Index<StoredClip>;
type PositionedId = Index<Positioned>;
type GeometryId = Index<GeometryRecord>;
type ResolvedClipId = Index<ResolvedClip>;

struct AtomKind<C> {
    type_id: TypeId,
    measure: fn(&DataArena, DataId, &mut C, Constraints) -> Size,
    paint_bounds: fn(&DataArena, DataId, Rect) -> Rect,
    paint: fn(&DataArena, DataId, &mut C, Rect),
}

type OverrideSize = fn(&mut DataArena, DataId, DataId, Option<f32>, Option<f32>) -> bool;
type StoreDefault = fn(&mut DataArena) -> DataId;

struct LayoutKind<C> {
    type_id: TypeId,
    layout: fn(&DataArena, &mut Frame<C>, NodeId, &mut C, DataId, Constraints) -> Size,
    override_size: OverrideSize,
    store_default: StoreDefault,
}

struct ClipKind<C> {
    type_id: TypeId,
    push: fn(&DataArena, DataId, &mut C, Rect),
    pop: fn(&DataArena, DataId, &mut C),
}

fn measure_atom<C, A: Atom<C>>(
    data: &DataArena,
    id: DataId,
    context: &mut C,
    constraints: Constraints,
) -> Size {
    data.load::<A>(id).measure(context, constraints)
}

fn paint_bounds_atom<C, A: Atom<C>>(data: &DataArena, id: DataId, area: Rect) -> Rect {
    data.load::<A>(id).paint_bounds(area)
}

fn paint_atom<C, A: Atom<C>>(data: &DataArena, id: DataId, context: &mut C, area: Rect) {
    data.load::<A>(id).paint(context, area)
}

fn push_clip<C, X: Clip<C>>(data: &DataArena, id: DataId, context: &mut C, area: Rect) {
    data.load::<X>(id).push(context, area)
}

fn pop_clip<C, X: Clip<C>>(data: &DataArena, id: DataId, context: &mut C) {
    data.load::<X>(id).pop(context)
}

// WidgetId already contains a hash
#[derive(Default)]
struct WidgetIdHasher(u64);

impl Hasher for WidgetIdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = value;
    }

    fn write(&mut self, _: &[u8]) {
        unreachable!("WidgetId hashes one u64")
    }
}
