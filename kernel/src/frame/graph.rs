pub struct Frame<C: Context> {
    nodes: Vec<StoredNode>,
    placements: Vec<Placement<C::Scalar>>,
    atoms: Vec<StoredAtom>,
    clips: Vec<StoredClip>,
    bounds: Vec<Bounds<C::Scalar>>,
    atom_kinds: Vec<AtomKind<C>>,
    layout_kinds: Vec<LayoutKind<C>>,
    clip_kinds: Vec<ClipKind<C>>,
    bump: BumpArena,
    scratch: BumpArena,
    paint_order: Vec<NodeId>,
    active_clips: Vec<ResolvedClipId>,
    interaction: interaction::InteractionState<C::Scalar>,
    /// ids passed to [`Ui::area`] during this build
    areas_requested: Vec<Id>,
    // requested areas from the previous layout, sorted by id
    areas_previous: Vec<(Id, Rect<C::Scalar>)>,
    animations: Vec<animation::AnimationState>,
    transitions: Vec<transition::TransitionState<C::Scalar>>,
    target_sizes: Vec<TargetSize<C::Scalar>>,
    timers: Vec<timer::TimerState>,
    input: Input<C::Scalar>,
    time: Duration,
    screen: Rect<C::Scalar>,
    resized: bool,
    frame_requested: bool,
    // the next frame updates interactions against this layout
    settle: bool,
    #[cfg(debug_assertions)]
    widget_ids: std::collections::HashSet<Id>,
}

impl<C: Context> Default for Frame<C> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            placements: Vec::new(),
            atoms: Vec::new(),
            clips: Vec::new(),
            bounds: Vec::new(),
            atom_kinds: Vec::new(),
            layout_kinds: Vec::new(),
            clip_kinds: Vec::new(),
            bump: BumpArena::default(),
            scratch: BumpArena::default(),
            paint_order: Vec::new(),
            active_clips: Vec::new(),
            interaction: interaction::InteractionState::default(),
            areas_previous: Vec::new(),
            areas_requested: Vec::new(),
            animations: Vec::new(),
            transitions: Vec::new(),
            target_sizes: Vec::new(),
            timers: Vec::new(),
            input: Input::None,
            time: Duration::ZERO,
            screen: Rect::default(),
            resized: false,
            frame_requested: true,
            settle: false,
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
        // interactions update against the previous layout, so every frame settles in one more
        // unless nothing but settling built it
        let settling = std::mem::take(&mut self.settle)
            && !self.has_pending_redraw()
            && matches!(input, Input::None)
            && self.screen.size() == frame.size
            && self.next_timer_deadline().is_none_or(|deadline| deadline > time);
        self.settle = !settling;
        self.frame_requested = false;
        #[cfg(debug_assertions)]
        generation::begin();
        self.nodes.clear();
        self.placements.clear();
        self.atoms.clear();
        self.clips.clear();
        self.bounds.clear();
        self.areas_requested.clear();
        self.bump.clear();
        for kind in &mut self.layout_kinds {
            kind.default_item = BumpId::NONE;
        }
        #[cfg(debug_assertions)]
        self.widget_ids.clear();
        self.paint_order.clear();
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
        self.interaction.begin(&input, time);

        let id = Id::new("blit frame root");
        let root = self.push_node(None, id);
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.prepare_scratch();
        let output = widget.build(Ui::new(self, context, &scratch, root));
        scratch.prepare_scratch();
        self.scratch = scratch;
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
        let mut bump = std::mem::take(&mut self.bump);
        bump.prepare_scratch();
        transition::resolve(
            self,
            &bump,
            context,
            NodeId::new(0),
            Constraints::tight(self.screen.size()),
            true,
        );
        position::resolve(self);
        paint::resolve_order(self, &bump);
        paint::resolve_clips(self, &bump);
        interaction::resolve(self);
        self.animations.retain(|animation| animation.seen);
        self.transitions.retain(|state| state.seen);
        self.timers.retain(|timer| timer.seen);
        bump.prepare_scratch();
        self.bump = bump;
    }

    /// paints the resolved graph and releases its retained values
    pub fn paint(&mut self, context: &mut C) {
        let mut bump = std::mem::take(&mut self.bump);
        paint::paint(self, &bump, context);
        bump.clear();
        self.bump = bump;
    }

    pub fn has_pending_redraw(&self) -> bool {
        self.frame_requested
            || self.settle
            || self.animations.iter().any(animation::AnimationState::is_active)
            || self.transitions.iter().any(transition::TransitionState::is_active)
    }

    pub fn next_timer_deadline(&self) -> Option<Duration> {
        self.timers
            .iter()
            .filter_map(timer::TimerState::deadline)
            .chain(self.interaction.deadline)
            .min()
    }

    pub fn request_frame(&mut self) {
        self.frame_requested = true;
    }

    /// returns the area from the current frame after layout
    pub fn area(&self, id: Id) -> Option<Rect<C::Scalar>> {
        self.nodes
            .iter()
            .enumerate()
            .find_map(|(index, node)| (node.widget_id == id).then_some(self.placements[index].area))
    }
}

impl<C: Context> Frame<C> {
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
                    sizing: |bump, id, axis| bump.load::<L>(id).sizing(axis),
                    default_item: BumpId::NONE,
                });
                self.layout_kinds.len() - 1
            });
        if self.layout_kinds[kind].default_item.offset().is_none() {
            self.layout_kinds[kind].default_item = self.bump.store(L::Item::default());
        }
        let kind = u16::try_from(kind).expect("too many layout kinds");
        let bump_id = self.bump.store(value);
        let node = &mut self.nodes[node.index()];
        node.layout = bump_id;
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
                    shape: |bump, id, area| bump.load::<X>(id).shape(area),
                    push: |bump, id, context, area| bump.load::<X>(id).push(context, area),
                    pop: |bump, id, context| bump.load::<X>(id).pop(context),
                });
                self.clip_kinds.len() - 1
            });
        let id = StoredClipId::new(self.clips.len());
        self.clips.push(StoredClip {
            kind: u16::try_from(kind).expect("too many clip types"),
            bump_id: self.bump.store(clip),
        });
        id
    }

    fn push_node(&mut self, parent: Option<NodeId>, widget_id: Id) -> NodeId {
        let id = NodeId::new(self.nodes.len());
        self.nodes.push(StoredNode {
            widget_id,
            parent: parent.unwrap_or(id),
            visual_parent: parent.unwrap_or(id),
            subtree_end: id.value,
            first_atom: StoredAtomId::NONE,
            last_atom: StoredAtomId::NONE,
            layout: BumpId::NONE,
            layout_kind: 0,
            clip: StoredClipId::NONE,
            item: BumpId::NONE,
            relative: parent.unwrap_or(id),
            out_of_flow: false,
            interactive: false,
            bounds: BoundsId::NONE,
            resolved_clip: ResolvedClipId::NONE,
        });
        self.placements.push(Placement {
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

    fn bounds_mut(&mut self, node: NodeId) -> &mut Bounds<C::Scalar> {
        let index = if let Some(index) = self.nodes[node.index()].bounds.index() {
            index
        } else {
            let id = BoundsId::new(self.bounds.len());
            self.nodes[node.index()].bounds = id;
            self.bounds.push(Bounds {
                node,
                hit: Sides::all(C::Scalar::ZERO),
                transition: None,
            });
            id.index().unwrap()
        };
        &mut self.bounds[index]
    }

    fn clip_bounds(&self, clip: ResolvedClipId) -> Rect<C::Scalar> {
        clip.index()
            .map_or(self.screen, |clip| self.interaction.clips[clip].bounds)
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
    widget_id: Id,
    parent: NodeId,
    visual_parent: NodeId,
    subtree_end: u32,
    first_atom: StoredAtomId,
    last_atom: StoredAtomId,
    layout: BumpId,
    layout_kind: u16,
    clip: StoredClipId,
    item: BumpId,
    relative: NodeId,
    out_of_flow: bool,
    // requested an interaction kind
    interactive: bool,
    bounds: BoundsId,
    resolved_clip: ResolvedClipId,
}

#[derive(Clone, Copy)]
struct Placement<T> {
    area: Rect<T>,
    z_index: i16,
}

#[derive(Clone, Copy)]
struct TargetSize<T> {
    size: Size<T>,
    properties: crate::TransitionProperties,
}

#[derive(Clone, Copy)]
struct Bounds<T> {
    node: NodeId,
    hit: Sides<T>,
    transition: Option<Transition>,
}

#[derive(Clone, Copy)]
pub struct ResolvedClip<T> {
    parent: ResolvedClipId,
    depth: u32,
    clip: StoredClipId,
    area: Rect<T>,
    bounds: Rect<T>,
    shape: ClipShape<T>,
}

#[derive(Clone, Copy)]
struct StoredAtom {
    kind: u16,
    bump_id: BumpId,
    next: StoredAtomId,
}

#[derive(Clone, Copy)]
struct StoredClip {
    kind: u16,
    bump_id: BumpId,
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
type BoundsId = Index<Bounds<()>>;
type ResolvedClipId = Index<ResolvedClip<()>>;

struct AtomKind<C: Context> {
    type_id: TypeId,
    measure: fn(&BumpArena, BumpId, &mut C, Constraints<C::Scalar>) -> Size<C::Scalar>,
    paint_bounds: fn(&BumpArena, BumpId, Rect<C::Scalar>) -> Rect<C::Scalar>,
    paint: fn(&BumpArena, BumpId, &mut C, Rect<C::Scalar>),
}

struct LayoutKind<C: Context> {
    type_id: TypeId,
    run: for<'a> fn(&mut layout::LayoutCx<'a, C>, BumpId, Constraints<C::Scalar>) -> Size<C::Scalar>,
    sizing: fn(&BumpArena, BumpId, crate::Axis) -> crate::Sizing<C::Scalar>,
    default_item: BumpId,
}

struct ClipKind<C: Context> {
    type_id: TypeId,
    shape: fn(&BumpArena, BumpId, Rect<C::Scalar>) -> ClipShape<C::Scalar>,
    push: fn(&BumpArena, BumpId, &mut C, Rect<C::Scalar>),
    pop: fn(&BumpArena, BumpId, &mut C),
}
