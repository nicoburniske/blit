pub struct Frame<C> {
    inner: FrameInner,
    context: PhantomData<fn(&mut C)>,
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
        self.inner.frame_requested = false;

        let state = &mut self.inner;
        #[cfg(debug_assertions)]
        generation::begin();
        state.nodes.clear();
        state.atoms.clear();
        state.layouts.clear();
        state.clips.clear();
        state.geometry.clear();
        state.requests.clear();
        state.data.clear();
        // have to clear these bc data arena will be cleared
        for kind in &mut state.layout_kinds {
            kind.default_item = DataId::NONE;
        }
        #[cfg(debug_assertions)]
        state.widget_ids.clear();
        state.paint_order.clear();
        state.resolved_clips.clear();
        state.active_clips.clear();
        state.input = input;
        state.time = time;
        state.resized = state.screen.size() != frame.size;
        state.screen = LogicalRect::new(0.0, 0.0, frame.size.width, frame.size.height);
        for animation in &mut state.animations {
            animation.seen = false;
        }
        for state in &mut state.transitions {
            state.seen = false;
        }
        for timer in &mut state.timers {
            timer.seen = false;
        }
        state.interaction.begin(&input);

        let output = {
            let id = WidgetId::new("blit frame root");
            let root = state.push_node(None, id);
            widget.build(Ui::new(&mut *context, &mut *self, root))
        };
        let state = &mut self.inner;
        #[cfg(debug_assertions)]
        assert!(
            state.nodes.iter().all(|node| state.widget_ids.insert(node.widget_id)),
            "widget ids must identify unique nodes"
        );
        assert_eq!(
            state.nodes[0].subtree_end as usize,
            state.nodes.len() - 1,
            "a frame must have exactly one root"
        );

        output
    }

    /// resolves layout, positioning, clipping and interaction for the built graph
    pub fn layout(&mut self, context: &mut C) {
        let frame = &mut self.inner;
        frame.scratch.rewind(0);
        let data = std::mem::take(&mut frame.data);
        let context = (context as *mut C).cast();
        transition::resolve(context, frame, &data, frame.screen.size(), frame.resized);
        paint::resolve_order(frame);
        paint::resolve_clips(frame);
        interaction::resolve(frame);
        frame.animations.retain(|animation| animation.seen);
        frame.transitions.retain(|state| state.seen);
        frame.timers.retain(|timer| timer.seen);
        frame.data = data;
    }

    /// paints the resolved graph and releases its retained values
    pub fn paint(&mut self, context: &mut C) {
        let frame = &mut self.inner;
        let mut data = std::mem::take(&mut frame.data);
        paint::render((context as *mut C).cast(), frame, &data);
        data.clear();
        frame.data = data;
    }

    pub fn has_pending_redraw(&self) -> bool {
        let frame = &self.inner;
        frame.frame_requested
            || frame.animations.iter().any(animation::AnimationState::is_active)
            || frame.transitions.iter().any(transition::TransitionState::is_active)
    }

    pub fn next_timer_deadline(&self) -> Option<Duration> {
        self.inner.timers.iter().filter_map(timer::TimerState::deadline).min()
    }

    pub fn request_frame(&mut self) {
        self.inner.frame_requested = true;
    }

    /// returns geometry from the current frame after layout
    pub fn geometry(&self, id: WidgetId) -> Option<LogicalRect> {
        self.inner
            .nodes
            .iter()
            .find_map(|node| (node.widget_id == id).then_some(node.area))
    }

    fn push_atom<A: Atom<C>>(&mut self, node: NodeId, atom: A) {
        let frame = &mut self.inner;
        let type_id = TypeId::of::<A>();
        let kind = frame
            .atom_kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                frame.atom_kinds.push(AtomKind {
                    type_id,
                    measure: measure_atom::<C, A>,
                    intrinsic: intrinsic_atom::<C, A>,
                    paint_bounds: paint_bounds_atom::<C, A>,
                    paint: paint_atom::<C, A>,
                });
                frame.atom_kinds.len() - 1
            });
        let id = StoredAtomId::new(frame.atoms.len());
        frame.atoms.push(StoredAtom {
            kind: u16::try_from(kind).expect("too many atom types"),
            data: frame.data.store(atom),
            next: StoredAtomId::NONE,
        });
        let node = node.index();
        if let Some(last) = frame.nodes[node].last_atom.index() {
            frame.atoms[last].next = id;
        } else {
            frame.nodes[node].first_atom = id;
        }
        frame.nodes[node].last_atom = id;
    }

    fn store_clip<X: Clip<C>>(&mut self, clip: X) -> StoredClipId {
        let frame = &mut self.inner;
        let type_id = TypeId::of::<X>();
        let kind = frame
            .clip_kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                frame.clip_kinds.push(ClipKind {
                    type_id,
                    push: push_clip::<C, X>,
                    pop: pop_clip::<C, X>,
                });
                frame.clip_kinds.len() - 1
            });
        let id = StoredClipId::new(frame.clips.len());
        frame.clips.push(StoredClip {
            kind: u16::try_from(kind).expect("too many clip types"),
            data: frame.data.store(clip),
        });
        id
    }
}

impl<C> Default for Frame<C> {
    fn default() -> Self {
        Self {
            inner: FrameInner::default(),
            context: PhantomData,
        }
    }
}

// paired with the callbacks registered through Frame<C>
type Context = *mut ();

pub struct FrameInner {
    nodes: Vec<StoredNode>,
    atoms: Vec<StoredAtom>,
    layouts: Vec<StoredLayout>,
    clips: Vec<StoredClip>,
    geometry: Vec<GeometryRecord>,
    atom_kinds: Vec<AtomKind>,
    layout_kinds: Vec<LayoutKind>,
    clip_kinds: Vec<ClipKind>,
    data: DataArena,
    scratch: DataArena,
    paint_links: Vec<PaintLinks>,
    paint_order: Vec<NodeId>,
    order_stack: Vec<NodeId>,
    resolved_clips: Vec<ResolvedClip>,
    active_clips: Vec<ResolvedClipId>,
    interaction: interaction::InteractionState,
    geometry_previous: Vec<(WidgetId, LogicalRect)>,
    requests: HashMap<WidgetId, Request, BuildHasherDefault<WidgetIdHasher>>,
    animations: Vec<animation::AnimationState>,
    transitions: Vec<transition::TransitionState>,
    target_sizes: Vec<TargetSize>,
    timers: Vec<timer::TimerState>,
    input: Input,
    time: Duration,
    screen: LogicalRect,
    resized: bool,
    frame_requested: bool,
    #[cfg(debug_assertions)]
    widget_ids: std::collections::HashSet<WidgetId>,
}

#[derive(Clone, Copy)]
enum Request {
    Geometry,
    Interaction(Sense),
}

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

impl Default for FrameInner {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            atoms: Vec::new(),
            layouts: Vec::new(),
            clips: Vec::new(),
            geometry: Vec::new(),
            atom_kinds: Vec::new(),
            layout_kinds: Vec::new(),
            clip_kinds: Vec::new(),
            data: DataArena::default(),
            scratch: DataArena::default(),
            paint_links: Vec::new(),
            paint_order: Vec::new(),
            order_stack: Vec::new(),
            resolved_clips: Vec::new(),
            active_clips: Vec::new(),
            interaction: interaction::InteractionState::default(),
            geometry_previous: Vec::new(),
            requests: HashMap::default(),
            animations: Vec::new(),
            transitions: Vec::new(),
            target_sizes: Vec::new(),
            timers: Vec::new(),
            input: Input::None,
            time: Duration::ZERO,
            screen: LogicalRect::default(),
            resized: false,
            frame_requested: true,
            #[cfg(debug_assertions)]
            widget_ids: Default::default(),
        }
    }
}

impl FrameInner {
    #[inline(always)]
    fn layout_node(
        &mut self,
        context: Context,
        data: &DataArena,
        node: NodeId,
        mut constraints: Constraints,
    ) -> LogicalSize {
        let index = node.index();
        if !self.target_sizes.is_empty() {
            let current = self.nodes[index].area.size();
            let properties = self.target_sizes[index].properties;
            if properties.intersects(TransitionProperties::WIDTH) {
                constraints.min.width = current.width;
                constraints.max.width = current.width;
            }
            if properties.intersects(TransitionProperties::HEIGHT) {
                constraints.min.height = current.height;
                constraints.max.height = current.height;
            }
        }
        let size = if let Some(layout) = self.nodes[index].layout.index() {
            let stored = self.layouts[layout];
            let run = self.layout_kinds[stored.kind as usize].layout;
            run(context, self, data, node, stored.data, constraints)
        } else if constraints.min == constraints.max {
            constraints.min
        } else {
            constraints.constrain(self.measure_base(context, data, node, constraints))
        };
        self.nodes[index].area.width = size.width;
        self.nodes[index].area.height = size.height;
        size
    }

    #[inline]
    fn intrinsic_node(
        &mut self,
        context: Context,
        data: &DataArena,
        node: NodeId,
        query: crate::IntrinsicQuery,
    ) -> crate::IntrinsicSize {
        assert!(query.cross.is_none_or(|value| value.is_finite() && value >= 0.0));
        let result = if let Some(layout) = self.nodes[node.index()].layout.index() {
            let stored = self.layouts[layout];
            let run = self.layout_kinds[stored.kind as usize].intrinsic;
            run(context, self, data, node, stored.data, query)
        } else {
            let mut size = crate::IntrinsicSize::default();
            let mut atom = self.nodes[node.index()].first_atom;
            while let Some(index) = atom.index() {
                let stored = self.atoms[index];
                let intrinsic = self.atom_kinds[stored.kind as usize].intrinsic;
                let measured = intrinsic(context, data, stored.data, query);
                size.min = size.min.max(measured.min);
                size.preferred = size.preferred.max(measured.preferred);
                atom = stored.next;
            }
            size
        };
        assert!(
            result.min.is_finite()
                && result.min >= 0.0
                && result.preferred.is_finite()
                && result.preferred >= result.min,
            "invalid intrinsic size"
        );
        result
    }

    #[inline]
    fn measure_base(
        &mut self,
        context: Context,
        data: &DataArena,
        node: NodeId,
        constraints: Constraints,
    ) -> LogicalSize {
        let mut size = LogicalSize::ZERO;
        let mut atom = self.nodes[node.index()].first_atom;
        while let Some(index) = atom.index() {
            let stored = self.atoms[index];
            let measure = self.atom_kinds[stored.kind as usize].measure;
            let measured = measure(context, data, stored.data, constraints);
            size = size.max(measured);
            atom = stored.next;
        }
        size
    }

    fn store_layout<L: Layout>(&mut self, value: L) -> StoredLayoutId {
        let type_id = TypeId::of::<L>();
        let kind = self
            .layout_kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                self.layout_kinds.push(LayoutKind {
                    type_id,
                    layout: layout::run::<L>,
                    intrinsic: layout::intrinsic::<L>,
                    default_item: DataId::NONE,
                });
                self.layout_kinds.len() - 1
            });
        if self.layout_kinds[kind].default_item.offset().is_none() {
            self.layout_kinds[kind].default_item = self.data.store(L::Item::default());
        }
        let id = StoredLayoutId::new(self.layouts.len());
        self.layouts.push(StoredLayout {
            kind: u16::try_from(kind).expect("too many layout kinds"),
            data: self.data.store(value),
            offset: LogicalPoint::ZERO,
        });
        id
    }

    #[inline]
    fn push_node(&mut self, parent: Option<NodeId>, widget_id: WidgetId) -> NodeId {
        let id = NodeId::new(self.nodes.len());
        self.nodes.push(StoredNode {
            widget_id,
            relative: parent.unwrap_or(id),
            parent: parent.unwrap_or(id),
            subtree_end: id.value,
            first_atom: StoredAtomId::NONE,
            last_atom: StoredAtomId::NONE,
            layout: StoredLayoutId::NONE,
            clip: StoredClipId::NONE,
            item: DataId::NONE,
            area: LogicalRect::default(),
            out_of_flow: false,
            z_index: 0,
            geometry: GeometryId::NONE,
            resolved_clip: ResolvedClipId::NONE,
            #[cfg(debug_assertions)]
            layout_state: LayoutState::Unlaid,
        });
        id
    }

    fn resolve_target(&self, node: NodeId, target: NodeTarget) -> NodeId {
        let target = match target {
            NodeTarget::Parent => self.nodes[node.index()].relative,
            NodeTarget::Root => NodeId::new(0),
            NodeTarget::Node(id) => id,
            NodeTarget::Widget(id) => {
                let open = {
                    // open ancestors follow the positioning reference chain
                    let mut ancestor = self.nodes[node.index()].relative;
                    loop {
                        let stored = &self.nodes[ancestor.index()];
                        if stored.widget_id == id {
                            break Some(ancestor);
                        }
                        if stored.relative == ancestor {
                            break None;
                        }
                        ancestor = stored.relative;
                    }
                };
                if let Some(open) = open {
                    open
                } else {
                    // search recent closed nodes first
                    // todo: repeated lookups are n^2
                    let index = self.nodes[..node.index()]
                        .iter()
                        .rposition(|stored| stored.widget_id == id)
                        .expect("target widget id must already be assigned");
                    NodeId::new(index)
                }
            }
        };
        assert!(target.index() < node.index(), "target must be declared before its node");
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
            });
            id.index().unwrap()
        };
        &mut self.geometry[index]
    }

    fn layout_offset(&self, node: NodeId) -> LogicalPoint {
        self.nodes[node.index()]
            .layout
            .index()
            .map_or(LogicalPoint::ZERO, |layout| self.layouts[layout].offset)
    }

    fn clip_bounds(&self, clip: ResolvedClipId) -> LogicalRect {
        clip.index()
            .map_or(self.screen, |clip| self.resolved_clips[clip].bounds)
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
    widget_id: WidgetId,
    relative: NodeId,
    parent: NodeId,
    subtree_end: u32,
    first_atom: StoredAtomId,
    last_atom: StoredAtomId,
    layout: StoredLayoutId,
    clip: StoredClipId,
    item: DataId,
    area: LogicalRect,
    out_of_flow: bool,
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
struct TargetSize {
    size: LogicalSize,
    properties: TransitionProperties,
}

#[derive(Clone, Copy)]
struct GeometryRecord {
    node: NodeId,
    hit: Sides,
    transition: Option<Transition>,
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
    area: LogicalRect,
    bounds: LogicalRect,
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
    offset: LogicalPoint,
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
type GeometryId = Index<GeometryRecord>;
type ResolvedClipId = Index<ResolvedClip>;

struct AtomKind {
    type_id: TypeId,
    intrinsic: fn(Context, &DataArena, DataId, crate::IntrinsicQuery) -> crate::IntrinsicSize,
    measure: fn(Context, &DataArena, DataId, Constraints) -> LogicalSize,
    paint_bounds: fn(&DataArena, DataId, LogicalRect) -> LogicalRect,
    paint: fn(Context, &DataArena, DataId, LogicalRect),
}

struct LayoutKind {
    type_id: TypeId,
    layout: fn(Context, &mut FrameInner, &DataArena, NodeId, DataId, Constraints) -> LogicalSize,
    intrinsic: fn(Context, &mut FrameInner, &DataArena, NodeId, DataId, crate::IntrinsicQuery) -> crate::IntrinsicSize,
    default_item: DataId,
}

struct ClipKind {
    type_id: TypeId,
    push: fn(Context, &DataArena, DataId, LogicalRect),
    pop: fn(Context, &DataArena, DataId),
}

// Frame<C> supplies the context and registration pairs each callback with its stored type
fn intrinsic_atom<C, A: Atom<C>>(
    context: Context,
    data: &DataArena,
    id: DataId,
    query: crate::IntrinsicQuery,
) -> crate::IntrinsicSize {
    unsafe { data.load_unchecked::<A>(id) }.intrinsic(unsafe { &mut *context.cast::<C>() }, query)
}

fn measure_atom<C, A: Atom<C>>(
    context: Context,
    data: &DataArena,
    id: DataId,
    constraints: Constraints,
) -> LogicalSize {
    unsafe { data.load_unchecked::<A>(id) }.measure(unsafe { &mut *context.cast::<C>() }, constraints)
}

fn paint_bounds_atom<C, A: Atom<C>>(data: &DataArena, id: DataId, area: LogicalRect) -> LogicalRect {
    data.load::<A>(id).paint_bounds(area)
}

fn paint_atom<C, A: Atom<C>>(context: Context, data: &DataArena, id: DataId, area: LogicalRect) {
    data.load::<A>(id).paint(unsafe { &mut *context.cast::<C>() }, area)
}

fn push_clip<C, X: Clip<C>>(context: Context, data: &DataArena, id: DataId, area: LogicalRect) {
    data.load::<X>(id).push(unsafe { &mut *context.cast::<C>() }, area)
}

fn pop_clip<C, X: Clip<C>>(context: Context, data: &DataArena, id: DataId) {
    data.load::<X>(id).pop(unsafe { &mut *context.cast::<C>() })
}
