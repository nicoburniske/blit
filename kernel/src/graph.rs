//! stored nodes of the frame graph and the side tables they index

use crate::*;

/// identifies a node only during the current render
///
/// do not store this across renders
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(u32);

impl NodeId {
    #[inline]
    pub(crate) fn new(index: usize) -> Self {
        assert!(index < Slots::KEPT as usize, "too many frame nodes");
        Self(index as u32)
    }

    #[inline]
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy)]
pub enum FocusRequest {
    Keep,
    Clear,
    Focus(Id),
}

pub struct StoredNode {
    // how it was told apart from its siblings, which the next build matches against
    pub mark: Mark,
    pub id: Id,
    pub parent: NodeId,
    pub subtree_end: u32,
    pub first_measure: StoredMeasureId,
    pub last_paint: StoredPaintId,
    pub layout: BumpId,
    pub layout_kind: u16,
    pub aux: AuxId,
    pub place: PlaceId,
    pub item: BumpId,
    pub target: NodeId,
    pub interactive: bool,
}

// settings most nodes leave unset, kept apart so nodes stay small
#[derive(Clone, Copy)]
pub struct Aux<T> {
    pub visual_parent: NodeId,
    pub clip: Option<Corners<T>>,
    pub limits: Constraints<T>,
    pub padding: Sides<T>,
    pub hit: Sides<T>,
    pub motion: NodeMotionId,
}

// the parent a node stacks, clips and contains its size within
pub fn visual_parent<T>(aux: &[Aux<T>], stored: &StoredNode) -> NodeId {
    stored
        .aux
        .index()
        .map_or(stored.parent, |index| aux[index].visual_parent)
}

#[derive(Clone, Copy, Default)]
pub struct Placement<T> {
    pub area: Rect<T>,
    pub z_index: i16,
}

#[derive(Clone, Copy)]
pub struct NodeMotion {
    pub node: NodeId,
    pub motion: Motion,
}

#[derive(Clone, Copy)]
pub struct ResolvedClip<T> {
    pub parent: ResolvedClipId,
    pub depth: u32,
    pub area: Rect<T>,
    pub radius: Corners<T>,
    pub bounds: Rect<T>,
}

pub struct StoredMeasure<C: Context> {
    pub run: fn(&BumpArena, BumpId, &mut C, Constraints<C::Scalar>) -> Size<C::Scalar>,
    pub bump_id: BumpId,
    pub next: StoredMeasureId,
}

impl<C: Context> Clone for StoredMeasure<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: Context> Copy for StoredMeasure<C> {}

pub struct StoredPaint<C: Context> {
    pub run: fn(&BumpArena, BumpId, &mut C, Rect<C::Scalar>),
    pub bump_id: BumpId,
    pub next: StoredPaintId,
}

impl<C: Context> Clone for StoredPaint<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: Context> Copy for StoredPaint<C> {}

pub struct StoredPlace<C: Context> {
    pub bump_id: BumpId,
    pub run: for<'a> fn(&mut PlaceCx<'a, C>, BumpId, Rect<C::Scalar>) -> Point<C::Scalar>,
}

impl<C: Context> Clone for StoredPlace<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C: Context> Copy for StoredPlace<C> {}

#[derive(Clone, Copy)]
pub struct Index<T>(pub u32, PhantomData<fn() -> T>);

impl<T> Index<T> {
    pub const NONE: Self = Self(u32::MAX, PhantomData);

    pub fn new(index: usize) -> Self {
        Self(u32::try_from(index).expect("too many frame values"), PhantomData)
    }

    pub fn index(&self) -> Option<usize> {
        (self.0 != u32::MAX).then_some(self.0 as usize)
    }
}

pub type StoredMeasureId = Index<StoredMeasure<()>>;
pub type StoredPaintId = Index<StoredPaint<()>>;
pub type AuxId = Index<Aux<()>>;
pub type PlaceId = Index<StoredPlace<()>>;
pub type NodeMotionId = Index<NodeMotion>;
pub type ResolvedClipId = Index<ResolvedClip<()>>;

pub struct LayoutKind<C: Context> {
    pub type_id: TypeId,
    pub run: for<'a> fn(&mut LayoutCx<'a, C>, BumpId, Constraints<C::Scalar>) -> Size<C::Scalar>,
    pub default_item: BumpId,
}

/// ids of the nodes in the current and previous build
///
/// a node inherits the id of the node it matches in the previous build, or takes a dead slot.
/// a slot lives while the current or previous build used it, by building its node or by naming
/// it before or without building it. once a whole build leaves it unused it is dead, and taking
/// it again moves its generation on so ids kept elsewhere stop resolving.
///
/// slots remember the build that last used them, so a build clears and sweeps nothing
#[derive(Default)]
pub struct Slots {
    slots: Vec<Slot>,
    names: Names,
    build: u32,
    // where the search for a dead slot resumes in this build
    sweep: u32,
}

#[derive(Clone, Copy)]
struct Slot {
    generation: NonZeroU32,
    // the last build that used it
    build: u32,
    // its node in that build or KEPT
    node: u32,
    // its node in the build before or KEPT / NONE
    old: u32,
}

impl Slots {
    /// no node, as for a slot without one or a node new to this build
    pub const NONE: u32 = u32::MAX;
    // slot kept through the build without a node
    const KEPT: u32 = u32::MAX - 1;

    /// starts a build, so the current nodes become the previous build's
    pub fn begin(&mut self) {
        self.build = self.build.wrapping_add(1);
        self.sweep = 0;
        if self.slots.is_empty() {
            self.alloc();
        }
    }

    /// whether another node of this build already holds `id`
    #[inline]
    pub fn claimed(&self, id: Id) -> bool {
        let slot = &self.slots[id.slot as usize];
        slot.build == self.build && slot.node < Self::KEPT
    }

    /// assigns `id` to `node`
    #[inline]
    pub fn set(&mut self, id: Id, node: NodeId) {
        self.touch(id.slot as usize).node = node.0;
    }

    /// keeps `id` through this build without a node
    #[inline]
    pub fn keep(&mut self, id: Id) {
        if self.live(id) {
            self.touch(id.slot as usize);
        }
    }

    /// whether `id` is live and already used in this build
    #[inline]
    pub fn is_kept(&self, id: Id) -> bool {
        self.live(id) && self.slots[id.slot as usize].build == self.build
    }

    pub fn alloc(&mut self) -> Id {
        let build = self.build;
        // slots before the sweep stay live through this build, as only a new build kills them
        let mut index = self.sweep as usize;
        while self
            .slots
            .get(index)
            .is_some_and(|slot| build.wrapping_sub(slot.build) <= 1)
        {
            index += 1;
        }
        self.sweep = u32::try_from(index + 1).expect("too many ids");
        let generation = self.slots.get(index).map_or(NonZeroU32::MIN, |slot| {
            slot.generation.checked_add(1).unwrap_or(NonZeroU32::MIN)
        });
        let slot = Slot {
            generation,
            build,
            node: Self::KEPT,
            old: Self::NONE,
        };
        if index == self.slots.len() {
            self.slots.push(slot);
        } else {
            self.slots[index] = slot;
        }
        Id {
            slot: index as u32,
            generation,
        }
    }

    /// the node holding `id` in the current build
    #[inline]
    pub fn node(&self, id: Id) -> Option<NodeId> {
        let slot = self.slots.get(id.slot as usize)?;
        (slot.generation == id.generation && slot.build == self.build && slot.node < Self::KEPT)
            .then_some(NodeId(slot.node))
    }

    /// the node holding `id` in the previous build
    #[inline]
    pub fn old(&self, id: Id) -> Option<usize> {
        let slot = self.slots.get(id.slot as usize).filter(|_| self.live(id))?;
        // a slot this build has not used yet still holds its node from the previous one
        let old = if slot.build == self.build { slot.old } else { slot.node };
        (old < Self::KEPT).then_some(old as usize)
    }

    /// the id keyed `key` in `scope`, taking a slot when nothing holds the name yet
    pub fn name(&mut self, scope: Id, key: Key) -> Id {
        let mut bucket = 0;
        if !self.names.table.is_empty() {
            bucket = self.names.probe(scope, key);
            let index = self.names.table[bucket];
            if index != Names::EMPTY {
                let id = self.names.entries[index as usize].id;
                if self.live(id) {
                    return id;
                }
                // a dead entry for the name takes the new id
                let id = self.alloc();
                self.names.entries[index as usize].id = id;
                return id;
            }
        }
        let id = self.alloc();
        if self.names.entries.len() + 1 > self.names.table.len() / 2 {
            // dead names go once the table fills, so it grows only for live ones
            let mut entries = core::mem::take(&mut self.names.entries);
            entries.retain(|name| self.live(name.id));
            entries.push(Name { scope, key, id });
            self.names.entries = entries;
            self.names.rebuild();
        } else {
            let index = u32::try_from(self.names.entries.len()).expect("too many names");
            self.names.entries.push(Name { scope, key, id });
            self.names.table[bucket] = index;
        }
        id
    }

    /// the id keyed `key` in `scope`, if anything holds the name
    pub fn find(&self, scope: Id, key: Key) -> Option<Id> {
        // entries of dead slots stay until the table fills
        let index = (!self.names.table.is_empty()).then(|| self.names.table[self.names.probe(scope, key)])?;
        let id = self.names.entries.get(index as usize)?.id;
        self.live(id).then_some(id)
    }

    #[inline]
    fn live(&self, id: Id) -> bool {
        self.slots
            .get(id.slot as usize)
            .is_some_and(|slot| slot.generation == id.generation && self.build.wrapping_sub(slot.build) <= 1)
    }

    // marks a live slot used in this build, so its node so far is the previous build's
    #[inline]
    fn touch(&mut self, index: usize) -> &mut Slot {
        let build = self.build;
        let slot = &mut self.slots[index];
        if slot.build != build {
            slot.old = slot.node;
            slot.node = Self::KEPT;
            slot.build = build;
        }
        slot
    }
}

/// a node's place in the previous build and its child there to try first
#[derive(Clone, Copy)]
pub struct Cursor {
    pub old: u32,
    pub next: u32,
}

/// children built from one call site under an open node
#[derive(Clone, Copy)]
pub struct Site {
    pub location: &'static Location<'static>,
    /// the ordinal of its latest child
    pub ordinal: u32,
    /// whether its next child may still be among the previous children
    ///
    /// a site numbers its children from zero without gaps in every build, so once one of them
    /// matches nothing the later ones cannot match either, and skipping their search keeps a
    /// site that replaces another linear
    pub scan: bool,
}

/// how a node is told apart from its siblings
#[derive(Clone, Copy)]
pub enum Mark {
    /// built from a call site, repeats told apart by their order
    Site(&'static Location<'static>, u32),
    /// keyed in a scope already used in this build
    Key(u32, Key),
    /// repeating a key or keyed in an unused scope, found by name alone
    Named,
}

impl Mark {
    #[inline]
    pub fn is_site(self, location: &Location<'_>, ordinal: u32) -> bool {
        matches!(self, Self::Site(other, other_ordinal) if other_ordinal == ordinal && same_place(other, location))
    }
}

/// whether two locations are one call site, as copies of one may not share an address
#[inline]
pub fn same_place(site: &Location<'_>, other: &Location<'_>) -> bool {
    // out of line so other sites are told apart by line and column without a call
    #[cold]
    fn same_file(site: &Location<'_>, other: &Location<'_>) -> bool {
        site.file() == other.file()
    }
    core::ptr::eq(site, other)
        || (site.line() == other.line() && site.column() == other.column() && same_file(site, other))
}

/// ids of keyed nodes by scope and key, open addressed
///
/// buckets come from mixing the scope and key, entries compare exactly
#[derive(Default)]
struct Names {
    entries: Vec<Name>,
    // entry indices, at most half full
    table: Vec<u32>,
}

// a key in a scope and the id it names
#[derive(Clone, Copy)]
struct Name {
    scope: Id,
    key: Key,
    id: Id,
}

impl Names {
    const EMPTY: u32 = u32::MAX;

    fn rebuild(&mut self) {
        self.table.clear();
        if self.entries.is_empty() {
            return;
        }
        self.table
            // a quarter full so as many names again fit before the next compaction
            .resize((self.entries.len() * 4).next_power_of_two(), Self::EMPTY);
        for index in 0..self.entries.len() {
            let Name { scope, key, .. } = self.entries[index];
            let bucket = self.probe(scope, key);
            self.table[bucket] = index as u32;
        }
    }

    // the bucket holding the name, or the empty one where it would go
    fn probe(&self, scope: Id, key: Key) -> usize {
        let mask = self.table.len() - 1;
        let mixed = (key.0 ^ u64::from(scope.slot).rotate_left(32)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let mut bucket = (mixed >> (64 - self.table.len().trailing_zeros())) as usize;
        loop {
            let index = self.table[bucket];
            if index == Self::EMPTY {
                return bucket;
            }
            let name = self.entries[index as usize];
            if name.scope == scope && name.key == key {
                return bucket;
            }
            bucket = (bucket + 1) & mask;
        }
    }
}

/// a call site on a node, the key of its animations and timers
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Call {
    pub owner: Id,
    // compared by content as copies of one call site may not share an address
    pub location: &'static Location<'static>,
    // calls from one site repeat within a frame, so their order tells them apart
    pub repeat: u32,
}

impl Call {
    pub fn new(owner: Id, location: &'static Location<'static>) -> Self {
        Self {
            owner,
            location,
            repeat: 0,
        }
    }
}

impl Ord for Call {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // line and column first, as the file compares as a string
        let key = |call: &Self| {
            let location = call.location;
            (
                call.owner,
                location.line(),
                location.column(),
                location.file(),
                call.repeat,
            )
        };
        key(self).cmp(&key(other))
    }
}

impl PartialOrd for Call {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
