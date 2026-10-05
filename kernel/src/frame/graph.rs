pub struct Frame<C: Context> {
    nodes: Vec<StoredNode>,
    node_geometry: Vec<NodeGeometry<C::Scalar>>,
    atoms: Vec<StoredAtom>,
    clips: Vec<StoredClip>,
    geometry: Vec<GeometryRecord<C::Scalar>>,
    atom_kinds: Vec<AtomKind<C>>,
    layout_kinds: Vec<LayoutKind<C>>,
    clip_kinds: Vec<ClipKind<C>>,
    data: DataArena,
    paint_links: Vec<PaintLinks>,
    paint_order: Vec<NodeId>,
    order_stack: Vec<NodeId>,
    resolved_clips: Vec<ResolvedClip<C::Scalar>>,
    active_clips: Vec<ResolvedClipId>,
    interaction: interaction::InteractionState<C::Scalar>,
    geometry_previous: Vec<(WidgetId, Rect<C::Scalar>)>,
    requests: HashMap<WidgetId, Request, BuildHasherDefault<WidgetIdHasher>>,
    animations: Vec<animation::AnimationState>,
    transitions: Vec<transition::TransitionState<C::Scalar>>,
    target_sizes: Vec<TargetSize<C::Scalar>>,
    timers: Vec<timer::TimerState>,
    input: Input<C::Scalar>,
    time: Duration,
    screen: Rect<C::Scalar>,
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

impl<C: Context> Default for Frame<C> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            node_geometry: Vec::new(),
            atoms: Vec::new(),
            clips: Vec::new(),
            geometry: Vec::new(),
            atom_kinds: Vec::new(),
            layout_kinds: Vec::new(),
            clip_kinds: Vec::new(),
            data: DataArena::default(),
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
            screen: Rect::default(),
            resized: false,
            frame_requested: true,
            #[cfg(debug_assertions)]
            widget_ids: Default::default(),
        }
    }
}

impl<C: Context> Frame<C> {
    /// rebuilds the frame graph for one input
    pub fn build<W: Widget<C>>(
        &mut self,
        context: &mut C,
        frame: FrameInfo<C::Scalar>,
        time: Duration,
        input: Input<C::Scalar>,
        widget: W,
    ) -> W::Response {
        self.frame_requested = false;
        #[cfg(debug_assertions)]
        generation::begin();
        self.nodes.clear();
        self.node_geometry.clear();
        self.atoms.clear();
        self.clips.clear();
        self.geometry.clear();
        self.requests.clear();
        self.data.clear();
        for kind in &mut self.layout_kinds {
            kind.default_item = DataId::NONE;
        }
        #[cfg(debug_assertions)]
        self.widget_ids.clear();
        self.paint_order.clear();
        self.resolved_clips.clear();
        self.active_clips.clear();
        self.input = input;
        self.time = time;
        let size = frame.size;
        self.resized = self.screen.size() != size;
        self.screen = Rect::new(C::Scalar::ZERO, C::Scalar::ZERO, size.width, size.height);
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

        let id = WidgetId::new("blit frame root");
        let root = self.push_node(None, id);
        let output = widget.build(Ui::new(self, context, root));
        #[cfg(debug_assertions)]
        assert!(
            self.nodes.iter().all(|node| self.widget_ids.insert(node.widget_id)),
            "widget ids must identify unique nodes"
        );
        assert_eq!(
            self.nodes[0].subtree_end as usize,
            self.nodes.len() - 1,
            "a frame must have exactly one root"
        );
        output
    }

    /// resolves layout, positioning, clipping and interaction for the built graph
    pub fn layout(&mut self, context: &mut C) {
        let mut data = std::mem::take(&mut self.data);
        data.prepare_scratch();
        transition::resolve(self, &data, context, self.screen.size(), self.resized);
        position::resolve(self);
        paint::resolve_order(self);
        paint::resolve_clips(self);
        interaction::resolve(self);
        self.animations.retain(|animation| animation.seen);
        self.transitions.retain(|state| state.seen);
        self.timers.retain(|timer| timer.seen);
        data.prepare_scratch();
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
            || self.animations.iter().any(animation::AnimationState::is_active)
            || self.transitions.iter().any(transition::TransitionState::is_active)
    }

    pub fn next_timer_deadline(&self) -> Option<Duration> {
        self.timers.iter().filter_map(timer::TimerState::deadline).min()
    }

    pub fn request_frame(&mut self) {
        self.frame_requested = true;
    }

    /// returns geometry from the current frame after layout
    pub fn geometry(&self, id: WidgetId) -> Option<Rect<C::Scalar>> {
        self.nodes
            .iter()
            .enumerate()
            .find_map(|(index, node)| (node.widget_id == id).then_some(self.node_geometry[index].area))
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
                    measure: |data, id, context, constraints| data.load::<A>(id).measure(context, constraints),
                    paint_bounds: |data, id, area| data.load::<A>(id).paint_bounds(area),
                    paint: |data, id, context, area| data.load::<A>(id).paint(context, area),
                });
                self.atom_kinds.len() - 1
            });
        let id = StoredAtomId::new(self.atoms.len());
        self.atoms.push(StoredAtom {
            kind: u16::try_from(kind).expect("too many atom types"),
            data: self.data.store(atom),
            next: StoredAtomId::NONE,
        });
        let node = &mut self.nodes[node.index()];
        if let Some(last) = node.last_atom.index() {
            self.atoms[last].next = id;
        } else {
            node.first_atom = id;
        }
        node.last_atom = id;
    }

    fn store_layout<L: Layout<C>>(&mut self, node: NodeId, value: L) {
        let type_id = TypeId::of::<L>();
        let kind = self
            .layout_kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                self.layout_kinds.push(LayoutKind {
                    type_id,
                    run: layout::run::<C, L>,
                    default_item: DataId::NONE,
                });
                self.layout_kinds.len() - 1
            });
        if self.layout_kinds[kind].default_item.offset().is_none() {
            self.layout_kinds[kind].default_item = self.data.store(L::Item::default());
        }
        let kind = u16::try_from(kind).expect("too many layout kinds");
        let data = self.data.store(value);
        let node = &mut self.nodes[node.index()];
        node.layout = data;
        node.layout_kind = kind;
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
                    push: |data, id, context, area| data.load::<X>(id).push(context, area),
                    pop: |data, id, context| data.load::<X>(id).pop(context),
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

    fn push_node(&mut self, parent: Option<NodeId>, widget_id: WidgetId) -> NodeId {
        let id = NodeId::new(self.nodes.len());
        self.nodes.push(StoredNode {
            widget_id,
            parent: parent.unwrap_or(id),
            visual_parent: parent.unwrap_or(id),
            subtree_end: id.value,
            first_atom: StoredAtomId::NONE,
            last_atom: StoredAtomId::NONE,
            layout: DataId::NONE,
            layout_kind: 0,
            clip: StoredClipId::NONE,
            item: DataId::NONE,
            relative: parent.unwrap_or(id),
            out_of_flow: false,
            geometry: GeometryId::NONE,
            resolved_clip: ResolvedClipId::NONE,
        });
        self.node_geometry.push(NodeGeometry {
            area: Rect::default(),
            z_index: 0,
        });
        id
    }

    fn resolve_target(&self, node: NodeId, target: NodeTarget) -> NodeId {
        let target = match target {
            NodeTarget::Parent => self.nodes[node.index()].parent,
            NodeTarget::Root => NodeId::new(0),
            NodeTarget::Node(id) => id,
            NodeTarget::Widget(id) => {
                let open = {
                    // open ancestors are cheap to find on the parent chain
                    let mut ancestor = self.nodes[node.index()].parent;
                    loop {
                        let stored = &self.nodes[ancestor.index()];
                        if stored.widget_id == id {
                            break Some(ancestor);
                        }
                        if stored.parent == ancestor {
                            break None;
                        }
                        ancestor = stored.parent;
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

    fn geometry_mut(&mut self, node: NodeId) -> &mut GeometryRecord<C::Scalar> {
        let index = if let Some(index) = self.nodes[node.index()].geometry.index() {
            index
        } else {
            let id = GeometryId::new(self.geometry.len());
            self.nodes[node.index()].geometry = id;
            self.geometry.push(GeometryRecord {
                node,
                hit: Sides::all(C::Scalar::ZERO),
                transition: None,
            });
            id.index().unwrap()
        };
        &mut self.geometry[index]
    }

    fn clip_bounds(&self, clip: ResolvedClipId) -> Rect<C::Scalar> {
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

struct StoredNode {
    widget_id: WidgetId,
    parent: NodeId,
    visual_parent: NodeId,
    subtree_end: u32,
    first_atom: StoredAtomId,
    last_atom: StoredAtomId,
    layout: DataId,
    layout_kind: u16,
    clip: StoredClipId,
    item: DataId,
    relative: NodeId,
    out_of_flow: bool,
    geometry: GeometryId,
    resolved_clip: ResolvedClipId,
}

#[derive(Clone, Copy)]
struct NodeGeometry<T> {
    area: Rect<T>,
    z_index: i16,
}

#[derive(Clone, Copy)]
struct TargetSize<T> {
    size: Size<T>,
    properties: crate::TransitionProperties,
}

#[derive(Clone, Copy)]
struct GeometryRecord<T> {
    node: NodeId,
    hit: Sides<T>,
    transition: Option<Transition>,
}

#[derive(Clone, Copy, Default)]
struct PaintLinks {
    first_child: u32,
    next_sibling: u32,
}

#[derive(Clone, Copy)]
struct ResolvedClip<T> {
    parent: ResolvedClipId,
    depth: u32,
    clip: StoredClipId,
    area: Rect<T>,
    bounds: Rect<T>,
}

#[derive(Clone, Copy)]
struct StoredAtom {
    kind: u16,
    data: DataId,
    next: StoredAtomId,
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

    fn index(&self) -> Option<usize> {
        (self.0 != u32::MAX).then_some(self.0 as usize)
    }
}

type StoredAtomId = Index<StoredAtom>;
type StoredClipId = Index<StoredClip>;
type GeometryId = Index<GeometryRecord<()>>;
type ResolvedClipId = Index<ResolvedClip<()>>;

struct AtomKind<C: Context> {
    type_id: TypeId,
    measure: fn(&DataArena, DataId, &mut C, Constraints<C::Scalar>) -> Size<C::Scalar>,
    paint_bounds: fn(&DataArena, DataId, Rect<C::Scalar>) -> Rect<C::Scalar>,
    paint: fn(&DataArena, DataId, &mut C, Rect<C::Scalar>),
}

struct LayoutKind<C: Context> {
    type_id: TypeId,
    run: for<'a> fn(&mut layout::LayoutCx<'a, C>, DataId, Constraints<C::Scalar>) -> Size<C::Scalar>,
    default_item: DataId,
}

struct ClipKind<C: Context> {
    type_id: TypeId,
    push: fn(&DataArena, DataId, &mut C, Rect<C::Scalar>),
    pop: fn(&DataArena, DataId, &mut C),
}
