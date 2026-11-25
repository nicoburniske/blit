//! a small immediate-mode kernel for building user interfaces
//!
//! # frame
//!
//! each frame rebuilds the interface as a tree of nodes in three stages
//!
//! 1. [`Kernel::build`] records nodes and registers callbacks for the later stages
//!    - [`Ui::measure`] sizes a node's content
//!    - [`Ui::layout`] arranges a node's children
//!    - [`Ui::paint`] draws a node's content
//! 2. [`Kernel::layout`] runs the measure and layout callbacks to size and position every node
//! 3. [`Kernel::paint`] runs the paint callbacks with resolved geometry
//!
//! # traits
//!
//! | trait        | stage         | does                                           |
//! |--------------|---------------|------------------------------------------------|
//! | [`Widget`]   | build         | describes nodes, layouts, content and children |
//! | [`Layout`]   | layout        | sizes and positions a node's children          |
//! | [`Place`]    | layout        | positions a node outside its parent's flow     |
//! | [`Interact`] | build         | turns input into a response for a widget       |
//!
//! # context
//!
//! everything is generic over a [`Context`], usually your renderer. it opts into capabilities
//!
//! - [`Context::push_clip`] clips drawing for nodes built with [`Ui::clip`]
//! - [`Paint<T>`](Paint) draws command `T`. paint callbacks send commands through it
//! - [`HasFrameStore`] stores drawing inputs in a [`FrameStore`] until the context renders
//!
//! # storage
//!
//! - [`Scratch`] is a temporary slice for the current scope, reclaimed when it ends
//!   - [`Ui::scratch`]
//!   - [`LayoutCx::scratch`]
//!   - [`PaintCx::scratch`]
//! - [`FrameSlice`] is a slice of drawing inputs the context owns until it renders
//!   - [`Ui::store_mut`]

#![no_std]

extern crate alloc;

pub use self::{
    animation::{Animation, AnimationOptions, Easing, Motion, MotionProps, Spring, Tween},
    frame_store::{FrameSlice, FrameSliceBuilder, FrameStore},
    geometry::{
        Constraints, Corners, LogicalPoint, LogicalRect, LogicalSize, PhysicalPoint, PhysicalRect, PhysicalSize, Point,
        Rect, Scalar, Scale2, Sides, Size,
    },
    graph::NodeId,
    interact::{Contact, ContactId, ContactSource, Contacts, Interact, InteractCx, KeyboardEvent, Phase, ScrollEvent},
    layout::{Axis, Children, Layout, LayoutCx, Place, PlaceCx},
    paint::PaintCx,
    scratch::{Scratch, ScratchBuffer},
    timer::Timer,
    ui::{Ui, state},
};

mod animation;
mod arena;
mod frame_store;
mod geometry;
mod graph;
mod interact;
mod layout;
mod macros;
mod math;
mod paint;
mod scratch;
mod timer;
mod ui;

pub mod input;

#[doc(hidden)]
pub mod internal;

use alloc::vec::Vec;
use core::{
    any::TypeId,
    marker::PhantomData,
    num::NonZeroU32,
    ops::{BitOr, BitOrAssign},
    panic::Location,
    time::Duration,
};

use self::{
    animation::{AnimationState, MotionState, Playback},
    arena::{BumpArena, BumpId},
    graph::*,
    input::{Input, KeyInput, MouseButton, ScrollPhase, TouchPhase},
    interact::InteractionState,
    internal::{
        animator::Animator,
        bump::{Bump, Offset},
    },
    timer::TimerState,
};

/// resources shared by every widget, layout and painter in a frame
pub trait Context {
    /// coordinate type for all frame geometry
    type Scalar: Scalar;

    /// the node built at `location` repeats `key` in `scope` and takes a fresh id instead
    fn duplicate_key(&mut self, _scope: Scope, _key: Key, _location: &'static Location<'static>) {}

    /// clips drawing to a rounded rectangle until the matching pop
    fn push_clip(&mut self, _area: Rect<Self::Scalar>, _radius: Corners<Self::Scalar>) {}

    /// restores the clip from before the matching push
    fn pop_clip(&mut self) {}
}

impl Context for () {
    type Scalar = f32;
}

/// accepts one kind of drawing operation in frame coordinates
pub trait Paint<T>: Context {
    /// area supplies placement and dimensions while the operation defines any outsets
    fn paint(&mut self, area: Rect<Self::Scalar>, command: &T);
}

/// provides access to drawing inputs owned by a context
pub trait HasFrameStore {
    fn store(&self) -> &FrameStore;
    fn store_mut(&mut self) -> &mut FrameStore;
}

builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct FrameInfo<T> {
        new(size: Size<T>),
    }
}

/// builds part of the ui each frame and returns a response such as whether it was clicked
///
/// - by default a widget owns a node it can lay out
/// - [`state::Insert`] widgets add content like text or backgrounds to the node through [`Ui::insert`]
///   without changing the node itself
pub trait Widget<C: Context, S = state::Build> {
    type Response;

    fn build(self, ui: Ui<'_, C, S>) -> Self::Response;
}

/// stable identity for widgets, animations and timers, handed out by the kernel
///
/// each build matches a node to the previous build by its parent and call site, or by its key
/// in a scope, and the node keeps the id it matched. an id stops resolving once its node is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id {
    slot: u32,
    generation: NonZeroU32,
}

impl Id {
    /// the frame root
    pub const ROOT: Self = Self {
        slot: 0,
        generation: NonZeroU32::MIN,
    };
}

/// a space of keys, so keys from unrelated code never meet
///
/// [`Ui::scope`] makes one owned by a node and a call site, and [`Scope::ROOT`] belongs to the app
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Scope(Id);

impl Scope {
    /// the app's own scope, which widgets leave alone
    pub const ROOT: Self = Self(Id::ROOT);

    /// the node keyed `key` in this scope, which may be built later
    pub fn key(self, key: impl Into<Key>) -> Target {
        Target {
            scope: self.0,
            key: Some(key.into()),
        }
    }
}

/// points at a node by its id, or by its key in a scope before it is built
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    scope: Id,
    key: Option<Key>,
}

impl From<Id> for Target {
    fn from(id: Id) -> Self {
        Self { scope: id, key: None }
    }
}

// lets optional settings such as a motion lift take an id directly
impl From<Id> for Option<Target> {
    fn from(id: Id) -> Self {
        Some(id.into())
    }
}

/// tells keyed nodes apart within a [`Scope`], compared exactly
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(u64);

// integers widen first so one key names one node whatever its type
macro_rules! key_from_integer {
    ($($integer:ty),*) => {$(
        impl From<$integer> for Key {
            fn from(key: $integer) -> Self {
                Self(key as u64)
            }
        }
    )*};
}

key_from_integer!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);

impl<C: Context, F, O> Widget<C> for F
where
    F: FnOnce(Ui<'_, C>) -> O,
{
    type Response = O;

    fn build(self, ui: Ui<'_, C>) -> Self::Response {
        self(ui)
    }
}

impl<C: Context> Widget<C, state::Insert> for () {
    type Response = ();

    fn build(self, _: Ui<'_, C, state::Insert>) {}
}

pub struct Kernel<C: Context> {
    //
    // current build
    //
    nodes: Vec<StoredNode>,
    placements: Vec<Placement<C::Scalar>>,
    // each node's clip, resolved after layout
    resolved_clips: Vec<ResolvedClipId>,
    measures: Vec<StoredMeasure<C>>,
    paints: Vec<StoredPaint<C>>,
    aux: Vec<Aux<C::Scalar>>,
    places: Vec<StoredPlace<C>>,
    node_motions: Vec<NodeMotion>,
    /// nodes in paint order, empty when tree order
    paint_order: Vec<NodeId>,
    /// call sites of children under the open nodes
    sites: Vec<Site>,
    bump: BumpArena,
    scratch: ScratchBuffer,

    //
    // latest layout
    //
    old_nodes: Vec<StoredNode>,
    old_placements: Vec<Placement<C::Scalar>>,

    //
    // retained state
    //
    slots: Slots,
    interaction: InteractionState<C::Scalar>,
    animations: Vec<AnimationState>,
    // the scopes made by call sites, sorted by call
    scopes: Vec<CallScope>,
    motions: Vec<MotionState<C::Scalar>>,
    timers: Vec<TimerState>,
    /// focus requested during this build, applied once it resolves
    focus: FocusRequest,

    //
    // node kinds
    //
    layout_kinds: Vec<LayoutKind<C>>,

    //
    // frame inputs
    //
    input: Input<C::Scalar>,
    time: Duration,
    screen: Rect<C::Scalar>,
    resized: bool,
    frame_requested: bool,
    /// the next frame updates interactions against this layout
    settle: bool,
}

impl<C: Context> Default for Kernel<C> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            placements: Vec::new(),
            resolved_clips: Vec::new(),
            measures: Vec::new(),
            paints: Vec::new(),
            aux: Vec::new(),
            places: Vec::new(),
            node_motions: Vec::new(),
            paint_order: Vec::new(),
            sites: Vec::new(),
            bump: BumpArena::default(),
            scratch: ScratchBuffer::default(),

            old_nodes: Vec::new(),
            old_placements: Vec::new(),

            slots: Slots::default(),
            interaction: InteractionState::default(),
            animations: Vec::new(),
            scopes: Vec::new(),
            motions: Vec::new(),
            timers: Vec::new(),
            focus: FocusRequest::Keep,

            layout_kinds: Vec::new(),

            input: Input::None,
            time: Duration::ZERO,
            screen: Rect::default(),
            resized: false,
            frame_requested: true,
            settle: false,
        }
    }
}

impl<C: Context> Kernel<C> {
    /// rebuilds the frame graph for one input
    ///
    /// call [`Self::layout`] after every build, as the next build reads that layout
    pub fn build<W: Widget<C>>(
        &mut self,
        context: &mut C,
        info: FrameInfo<C::Scalar>,
        time: Duration,
        input: Input<C::Scalar>,
        widget: W,
    ) -> W::Response {
        // interactions update against the previous layout, so every frame settles in one more
        // unless nothing but settling built it
        let settling = match input {
            Input::None => {
                core::mem::take(&mut self.settle)
                    && !self.has_pending_redraw()
                    && self.screen.size() == info.size
                    && self.next_timer_deadline().is_none_or(|deadline| deadline > time)
            }
            _ => false,
        };
        self.settle = !settling;
        self.frame_requested = false;
        // interactions update against the previous build's nodes
        self.interaction.begin(&input, time, &self.nodes);
        core::mem::swap(&mut self.nodes, &mut self.old_nodes);
        core::mem::swap(&mut self.placements, &mut self.old_placements);
        self.nodes.clear();
        self.placements.clear();
        self.slots.begin();
        self.focus = FocusRequest::Keep;
        self.measures.clear();
        self.paints.clear();
        self.aux.clear();
        self.places.clear();
        self.node_motions.clear();
        self.sites.clear();
        self.bump.clear();
        for kind in &mut self.layout_kinds {
            kind.default_item = BumpId::NONE;
        }
        self.paint_order.clear();
        self.input = input;
        self.time = time;
        let size = info.size;
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

        let root = self.push_node(None, Id::ROOT, Mark::Named);
        let mut scratch = core::mem::take(&mut self.scratch);
        scratch.prepare();
        let old = self.slots.old(Id::ROOT).map_or(Slots::NONE, |node| node as u32);
        let output = widget.build(Ui::new(self, context, &scratch, root, old));
        scratch.prepare();
        self.scratch = scratch;
        // a scope this build did not make dies with the next one
        let slots = &self.slots;
        self.scopes.retain(|scope| slots.is_kept(scope.id));
        assert_eq!(
            self.nodes[0].subtree_end as usize,
            self.nodes.len() - 1,
            "a frame must have exactly one root"
        );
        output
    }

    /// resolves layout, positioning, clipping and interaction for the built graph
    pub fn layout(&mut self, context: &mut C) {
        let mut bump = core::mem::take(&mut self.bump);
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
            let id = self.nodes[node.index()].id;
            let flying = self
                .motions
                .binary_search_by_key(&id, |state| state.id)
                .is_ok_and(|state| self.motions[state].flying);
            // a target that was not built before the node leaves it with its own parent
            if flying && let Some(parent) = ui::resolve_target(self, node, lift) {
                self.aux_mut(node).visual_parent = parent;
                self.placements[node.index()].z_index = i16::MAX;
            }
        }
        paint::resolve_order(self, &bump);
        paint::resolve_clips(self);
        // interactions bind against this build
        interact::resolve(self);
        self.animations.retain(|animation| animation.seen);
        self.motions.retain(|state| state.seen);
        self.timers.retain(|timer| timer.seen);
        bump.storage.prepare();
        self.bump = bump;
    }

    /// paints the resolved graph and releases its retained values
    pub fn paint(&mut self, context: &mut C) {
        let mut bump = core::mem::take(&mut self.bump);
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

    /// returns the area of the node `target` in the latest layout
    ///
    /// valid after [`Self::layout`]
    pub fn area(&self, target: impl Into<Target>) -> Option<Rect<C::Scalar>> {
        let node = self.slots.node(self.find(target.into())?)?;
        self.placements.get(node.index()).map(|placement| placement.area)
    }
}

//
// internals
//

impl<C: Context> Kernel<C> {
    /// pushes a node and assigns `id` to it
    #[inline(always)]
    fn push_node(&mut self, parent: Option<NodeId>, id: Id, mark: Mark) -> NodeId {
        let node = NodeId::new(self.nodes.len());
        self.slots.set(id, node);
        self.nodes.push(StoredNode {
            mark,
            id,
            parent: parent.unwrap_or(node),
            subtree_end: node.index() as u32,
            first_measure: StoredMeasureId::NONE,
            last_paint: StoredPaintId::NONE,
            layout: BumpId::NONE,
            layout_kind: 0,
            aux: AuxId::NONE,
            place: PlaceId::NONE,
            item: BumpId::NONE,
            target: parent.unwrap_or(node),
            interactive: false,
        });
        node
    }

    /// the id of `target`, kept through this build and taking a slot for a key nothing holds yet
    fn resolve(&mut self, target: Target) -> Id {
        let id = match target.key {
            Some(key) => self.slots.name(target.scope, key),
            None => target.scope,
        };
        self.slots.keep(id);
        id
    }

    /// the id of `target`, if anything holds it
    fn find(&self, target: Target) -> Option<Id> {
        match target.key {
            Some(key) => self.slots.find(target.scope, key),
            None => Some(target.scope),
        }
    }

    fn aux_mut(&mut self, node: NodeId) -> &mut Aux<C::Scalar> {
        let stored = &mut self.nodes[node.index()];
        let index = match stored.aux.index() {
            Some(index) => index,
            None => {
                stored.aux = AuxId::new(self.aux.len());
                self.aux.push(Aux {
                    visual_parent: stored.parent,
                    clip: None,
                    limits: Constraints::loose(Size::uniform(C::Scalar::UNBOUNDED)),
                    padding: Sides::all(C::Scalar::ZERO),
                    hit: Sides::all(C::Scalar::ZERO),
                    motion: NodeMotionId::NONE,
                });
                self.aux.len() - 1
            }
        };
        &mut self.aux[index]
    }
}

#[cfg(doctest)]
#[doc = include_str!("../tests/compile_fail.md")]
mod compile_fail {}
