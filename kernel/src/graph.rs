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
        Self(u32::try_from(index).expect("too many frame nodes"))
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
    pub widget_id: Id,
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
    pub motion: NodeMotionId,
    pub resolved_clip: ResolvedClipId,
}

// settings most nodes leave unset, kept apart so nodes stay small
#[derive(Clone, Copy)]
pub struct Aux<T> {
    pub visual_parent: NodeId,
    pub clip: StoredClip,
    pub limits: Constraints<T>,
    pub padding: Sides<T>,
    pub hit: Sides<T>,
}

// the parent a node stacks, clips and contains its size within
pub fn visual_parent<T>(aux: &[Aux<T>], stored: &StoredNode) -> NodeId {
    stored
        .aux
        .index()
        .map_or(stored.parent, |index| aux[index].visual_parent)
}

#[derive(Clone, Copy)]
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
    pub clip: StoredClip,
    pub area: Rect<T>,
    pub bounds: Rect<T>,
    pub shape: ClipShape<T>,
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
pub struct StoredClip {
    pub kind: u16,
    pub bump_id: BumpId,
}

impl StoredClip {
    pub const NONE: Self = Self {
        kind: 0,
        bump_id: BumpId::NONE,
    };
}

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

pub struct ClipKind<C: Context> {
    pub type_id: TypeId,
    pub shape: fn(&BumpArena, BumpId, Rect<C::Scalar>) -> ClipShape<C::Scalar>,
    pub push: fn(&BumpArena, BumpId, &mut C, Rect<C::Scalar>),
    pub pop: fn(&BumpArena, BumpId, &mut C),
}

/// indices of items keyed by their id, open addressed
///
/// ids are hashes already, so their low bits pick a slot. only indices are stored,
/// the caller's items hold the ids.
#[derive(Default)]
pub struct IdTable {
    slots: Vec<u32>,
}

impl IdTable {
    const EMPTY: u32 = u32::MAX;

    /// replaces the contents with `ids` at their positions
    pub fn rebuild(&mut self, ids: impl ExactSizeIterator<Item = Id>) {
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
    pub fn get(&self, id: Id, id_of: impl Fn(usize) -> Id) -> Option<usize> {
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
