pub mod animation;
pub mod arena;
pub mod atom;
pub mod interact;
pub mod layout;
pub mod paint;
pub mod timer;
pub mod ui;

use std::{any::TypeId, marker::PhantomData, mem::size_of, num::NonZeroU32, panic::Location, time::Duration};

use self::{
    animation::{AnimationState, MotionState, Playback},
    arena::{BumpArena, BumpId},
    timer::{Timer, TimerState},
    ui::{ROOT, Site, Target, Ui},
};
use crate::{
    Atom, AtomWidget, Clip, ClipShape, ContactId, ContactSource, Context, FrameInfo, FrameStore, HasFrameStore, Id,
    IntoWidget, Scalar, ScratchBuffer, Widget,
    animation::{Animation, AnimationOptions, Motion, MotionProps, Tween},
    geometry::{Constraints, Point, Rect, Sides, Size},
    input::{Input, MouseButton, ScrollPhase, TouchPhase},
    interact::{Interact, Phase, ScrollEvent},
    internal::{animator::Animator, bump::Bump},
    layout::{Axis, Layout, Place},
};

pub struct Frame<C: Context> {
    nodes: Vec<StoredNode>,
    placements: Vec<Placement<C::Scalar>>,
    atoms: Vec<StoredAtom<C>>,
    aux: Vec<Aux<C::Scalar>>,
    places: Vec<StoredPlace<C>>,
    node_motions: Vec<NodeMotion>,
    layout_kinds: Vec<LayoutKind<C>>,
    clip_kinds: Vec<ClipKind<C>>,
    bump: BumpArena,
    scratch: ScratchBuffer,
    paint_order: Vec<NodeId>,
    active_clips: Vec<ResolvedClipId>,
    interaction: interact::InteractionState<C::Scalar>,
    // focus requested during this build, applied once it resolves
    focus: FocusRequest,
    // the latest layout, which the next build reads areas from
    old_nodes: Vec<StoredNode>,
    old_placements: Vec<Placement<C::Scalar>>,
    // nodes of the latest layout by id
    laid_out: IdTable,
    animations: Vec<AnimationState>,
    motions: Vec<MotionState<C::Scalar>>,
    /// animated sizes in the subtree being resolved
    animated_sizes: Vec<MotionProps>,
    timers: Vec<TimerState>,
    // call sites of children under the open nodes
    sites: Vec<Site>,
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
            aux: Vec::new(),
            places: Vec::new(),
            node_motions: Vec::new(),
            layout_kinds: Vec::new(),
            clip_kinds: Vec::new(),
            bump: BumpArena::default(),
            scratch: ScratchBuffer::default(),
            paint_order: Vec::new(),
            active_clips: Vec::new(),
            interaction: interact::InteractionState::default(),
            focus: FocusRequest::Keep,
            old_nodes: Vec::new(),
            old_placements: Vec::new(),
            laid_out: IdTable::default(),
            animations: Vec::new(),
            motions: Vec::new(),
            animated_sizes: Vec::new(),
            timers: Vec::new(),
            sites: Vec::new(),
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
        let settling = match input {
            Input::None => {
                std::mem::take(&mut self.settle)
                    && !self.has_pending_redraw()
                    && self.screen.size() == frame.size
                    && self.next_timer_deadline().is_none_or(|deadline| deadline > time)
            }
            _ => false,
        };
        self.settle = !settling;
        self.frame_requested = false;
        #[cfg(debug_assertions)]
        generation::begin();
        // interactions update against the previous build's nodes
        self.interaction.begin(&input, time, &self.nodes);
        std::mem::swap(&mut self.nodes, &mut self.old_nodes);
        std::mem::swap(&mut self.placements, &mut self.old_placements);
        self.nodes.clear();
        self.placements.clear();
        self.focus = FocusRequest::Keep;
        self.atoms.clear();
        self.aux.clear();
        self.places.clear();
        self.node_motions.clear();
        self.sites.clear();
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
        for state in &mut self.motions {
            state.seen = false;
        }
        for timer in &mut self.timers {
            timer.seen = false;
        }

        let root = self.push_node(None, ROOT);
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.prepare();
        let output = widget.build(Ui::new(self, context, &scratch, root));
        scratch.prepare();
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
        bump.storage.prepare();
        layout::resolve(
            self,
            &bump,
            context,
            NodeId::new(0),
            Constraints::tight(self.screen.size()),
            true,
        );
        layout::position(self);
        for state in &mut self.motions {
            state.flying &= state.is_active();
        }
        // a lifted node paints above everything under its lift target while it flies to a new reference
        for index in 0..self.node_motions.len() {
            let NodeMotion {
                node,
                motion: Motion { lift: Some(lift), .. },
            } = self.node_motions[index]
            else {
                continue;
            };
            let id = self.nodes[node.index()].widget_id;
            let flying = self
                .motions
                .binary_search_by_key(&id, |state| state.id)
                .is_ok_and(|state| self.motions[state].flying);
            if flying {
                self.aux_mut(node).visual_parent = self.resolve_target(node, lift);
                self.placements[node.index()].z_index = i16::MAX;
            }
        }
        paint::resolve_order(self, &bump);
        paint::resolve_clips(self, &bump);
        // interactions bind against this build
        self.laid_out.rebuild(self.nodes.iter().map(|stored| stored.widget_id));
        interact::resolve(self);
        self.animations.retain(|animation| animation.seen);
        self.motions.retain(|state| state.seen);
        self.timers.retain(|timer| timer.seen);
        bump.storage.prepare();
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
            || self.animations.iter().any(AnimationState::is_active)
            || self.motions.iter().any(MotionState::is_active)
    }

    pub fn next_timer_deadline(&self) -> Option<Duration> {
        self.timers
            .iter()
            .filter_map(TimerState::deadline)
            .chain(self.interaction.deadline)
            .min()
    }

    pub fn request_frame(&mut self) {
        self.frame_requested = true;
    }

    /// returns the area of the node `target` names from the root in the latest layout
    pub fn area(&self, target: impl Into<Target>) -> Option<Rect<C::Scalar>> {
        let id = self.target_id(NodeId::new(0), target.into());
        self.laid_out
            .get(id, |index| self.nodes[index].widget_id)
            .map(|index| self.placements[index].area)
    }
}

/// identifies a node only during the current render
///
/// do not store this across renders
#[cfg_attr(not(debug_assertions), repr(transparent))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId {
    value: u32,
    #[cfg(debug_assertions)]
    generation: u16,
}

//
// internals
//

impl NodeId {
    #[inline]
    fn new(index: usize) -> Self {
        Self {
            value: u32::try_from(index).expect("too many frame nodes"),
            #[cfg(debug_assertions)]
            generation: generation::get(),
        }
    }

    fn index(self) -> usize {
        #[cfg(debug_assertions)]
        generation::assert(self.generation);
        self.value as usize
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

    fn store_clip<X: Clip<C>>(&mut self, clip: X) -> StoredClip {
        let type_id = TypeId::of::<X>();
        if type_id == TypeId::of::<()>() {
            return StoredClip::NONE;
        }
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
        StoredClip {
            kind: u16::try_from(kind).expect("too many clip types"),
            bump_id: self.bump.store(clip),
        }
    }

    fn push_node(&mut self, parent: Option<NodeId>, widget_id: Id) -> NodeId {
        let id = NodeId::new(self.nodes.len());
        self.nodes.push(StoredNode {
            widget_id,
            parent: parent.unwrap_or(id),
            subtree_end: id.value,
            first_atom: StoredAtomId::NONE,
            last_atom: StoredAtomId::NONE,
            layout: BumpId::NONE,
            layout_kind: 0,
            aux: AuxId::NONE,
            place: PlaceId::NONE,
            item: BumpId::NONE,
            target: parent.unwrap_or(id),
            interactive: false,
            motion: NodeMotionId::NONE,
            resolved_clip: ResolvedClipId::NONE,
        });
        self.placements.push(Placement {
            area: Rect::default(),
            z_index: 0,
        });
        id
    }

    fn aux_mut(&mut self, node: NodeId) -> &mut Aux<C::Scalar> {
        let stored = &mut self.nodes[node.index()];
        let index = match stored.aux.index() {
            Some(index) => index,
            None => {
                stored.aux = AuxId::new(self.aux.len());
                self.aux.push(Aux {
                    visual_parent: stored.parent,
                    clip: StoredClip::NONE,
                    limits: Constraints::loose(Size::uniform(C::Scalar::UNBOUNDED)),
                    padding: Sides::all(C::Scalar::ZERO),
                    hit: Sides::all(C::Scalar::ZERO),
                });
                self.aux.len() - 1
            }
        };
        &mut self.aux[index]
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

#[derive(Clone, Copy)]
enum FocusRequest {
    Keep,
    Clear,
    Focus(Id),
}

pub struct StoredNode {
    widget_id: Id,
    parent: NodeId,
    subtree_end: u32,
    first_atom: StoredAtomId,
    last_atom: StoredAtomId,
    layout: BumpId,
    layout_kind: u16,
    aux: AuxId,
    place: PlaceId,
    item: BumpId,
    target: NodeId,
    interactive: bool,
    motion: NodeMotionId,
    resolved_clip: ResolvedClipId,
}

// settings most nodes leave unset, kept apart so nodes stay small
#[derive(Clone, Copy)]
struct Aux<T> {
    visual_parent: NodeId,
    clip: StoredClip,
    limits: Constraints<T>,
    padding: Sides<T>,
    hit: Sides<T>,
}

// the parent a node stacks, clips and contains its size within
fn visual_parent<T>(aux: &[Aux<T>], stored: &StoredNode) -> NodeId {
    stored
        .aux
        .index()
        .map_or(stored.parent, |index| aux[index].visual_parent)
}

#[derive(Clone, Copy)]
struct Placement<T> {
    area: Rect<T>,
    z_index: i16,
}

#[derive(Clone, Copy)]
struct NodeMotion {
    node: NodeId,
    motion: Motion,
}

#[derive(Clone, Copy)]
pub struct ResolvedClip<T> {
    parent: ResolvedClipId,
    depth: u32,
    clip: StoredClip,
    area: Rect<T>,
    bounds: Rect<T>,
    shape: ClipShape<T>,
}

struct StoredAtom<C: Context> {
    kind: *const AtomKind<C>,
    bump_id: BumpId,
    next: StoredAtomId,
}

impl<C: Context> Clone for StoredAtom<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: Context> Copy for StoredAtom<C> {}

impl<C: Context> StoredAtom<C> {
    #[inline]
    fn kind(&self) -> &AtomKind<C> {
        // safety: kind points to the static table of an atom type
        unsafe { &*self.kind }
    }
}

struct StoredPlace<C: Context> {
    bump_id: BumpId,
    run: for<'a> fn(&mut layout::PlaceCx<'a, C>, BumpId, Rect<C::Scalar>) -> Point<C::Scalar>,
}

impl<C: Context> Clone for StoredPlace<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: Context> Copy for StoredPlace<C> {}

#[derive(Clone, Copy)]
struct StoredClip {
    kind: u16,
    bump_id: BumpId,
}

impl StoredClip {
    const NONE: Self = Self {
        kind: 0,
        bump_id: BumpId::NONE,
    };
}

#[derive(Clone, Copy)]
struct Index<T>(u32, PhantomData<fn() -> T>);

impl<T> Index<T> {
    const NONE: Self = Self(u32::MAX, PhantomData);

    fn new(index: usize) -> Self {
        Self(u32::try_from(index).expect("too many frame values"), PhantomData)
    }

    fn index(&self) -> Option<usize> {
        (self.0 != u32::MAX).then_some(self.0 as usize)
    }
}

type StoredAtomId = Index<StoredAtom<()>>;
type AuxId = Index<Aux<()>>;
type PlaceId = Index<StoredPlace<()>>;
type NodeMotionId = Index<NodeMotion>;
type ResolvedClipId = Index<ResolvedClip<()>>;

struct AtomKind<C: Context> {
    measure: fn(&BumpArena, BumpId, &mut C, Constraints<C::Scalar>) -> Size<C::Scalar>,
    paint: fn(&BumpArena, BumpId, &mut C, Rect<C::Scalar>),
}

struct LayoutKind<C: Context> {
    type_id: TypeId,
    run: for<'a> fn(&mut layout::LayoutCx<'a, C>, BumpId, Constraints<C::Scalar>) -> Size<C::Scalar>,
    default_item: BumpId,
}

struct ClipKind<C: Context> {
    type_id: TypeId,
    shape: fn(&BumpArena, BumpId, Rect<C::Scalar>) -> ClipShape<C::Scalar>,
    push: fn(&BumpArena, BumpId, &mut C, Rect<C::Scalar>),
    pop: fn(&BumpArena, BumpId, &mut C),
}

/// indices of items keyed by their id, open addressed
///
/// ids are hashes already, so their low bits pick a slot. only indices are stored,
/// the caller's items hold the ids.
#[derive(Default)]
struct IdTable {
    slots: Vec<u32>,
}

impl IdTable {
    const EMPTY: u32 = u32::MAX;

    /// replaces the contents with `ids` at their positions
    fn rebuild(&mut self, ids: impl ExactSizeIterator<Item = Id>) {
        // at most half full keeps probes short
        let len = (ids.len() * 2).next_power_of_two();
        self.slots.clear();
        self.slots.resize(len, Self::EMPTY);
        for (index, id) in ids.enumerate() {
            let mut slot = id.0.get() as usize & (len - 1);
            while self.slots[slot] != Self::EMPTY {
                slot = (slot + 1) & (len - 1);
            }
            self.slots[slot] = u32::try_from(index).expect("too many ids");
        }
    }

    /// the index of `id`, compared against the caller's items through `id_of`
    fn get(&self, id: Id, id_of: impl Fn(usize) -> Id) -> Option<usize> {
        let len = self.slots.len();
        if len == 0 {
            return None;
        }
        let mut slot = id.0.get() as usize & (len - 1);
        loop {
            let index = self.slots[slot];
            if index == Self::EMPTY {
                return None;
            }
            if id_of(index as usize) == id {
                return Some(index as usize);
            }
            slot = (slot + 1) & (len - 1);
        }
    }
}
