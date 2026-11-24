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
    ui::{Scope, Target, Ui, state},
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
    ui::{ROOT, Site},
};

/// resources shared by every widget, layout and painter in a frame
pub trait Context {
    /// coordinate type for all frame geometry
    type Scalar: Scalar;

    /// the node built at `location` repeats a key in its parent or scope, reported a frame later
    fn duplicate_id(&mut self, _id: Id, _location: &'static Location<'static>) {}

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

/// stable identity for widgets, animations and timers
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(core::num::NonZeroU64);

impl Id {
    /// creates an absolute id from a stable value
    pub fn new(source: impl core::hash::Hash) -> Self {
        use core::num::NonZeroU64;

        let mut hasher = internal::hash::FxHasher::default();
        source.hash(&mut hasher);
        Self(NonZeroU64::new(hasher.finish()).unwrap_or(NonZeroU64::MIN))
    }

    /// creates an absolute id from a name
    ///
    /// equivalent to hashing the byte slice with [`Self::new`]
    ///
    /// ```
    /// const PANEL: blit::Id = blit::Id::named(b"panel");
    /// ```
    pub const fn named(source: &[u8]) -> Self {
        let mut hasher = internal::hash::FxHasher::new();
        hasher.write_usize(source.len());
        hasher.write(source);
        match core::num::NonZeroU64::new(hasher.finish()) {
            Some(value) => Self(value),
            None => Self(core::num::NonZeroU64::MIN),
        }
    }

    /// composes a stable child id without using the current ui node
    pub fn child(self, source: impl core::hash::Hash) -> Self {
        Self::new((self, source))
    }
}

// integers widen first so one key names one id whatever its type
macro_rules! id_from_integer {
    ($($integer:ty),*) => {$(
        impl From<$integer> for Id {
            fn from(key: $integer) -> Self {
                Self::new(key as u64)
            }
        }
    )*};
}

id_from_integer!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);

impl From<&str> for Id {
    fn from(key: &str) -> Self {
        Self::new(key)
    }
}

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
    /// nodes by id
    laid_out: IdTable,
    /// an id found on several nodes, whose call sites the next build reports
    watch: Option<Id>,

    //
    // retained state
    //
    interaction: InteractionState<C::Scalar>,
    animations: Vec<AnimationState>,
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
            laid_out: IdTable::default(),
            watch: None,

            interaction: InteractionState::default(),
            animations: Vec::new(),
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

        let root = self.push_node(context, None, ROOT);
        let mut scratch = core::mem::take(&mut self.scratch);
        scratch.prepare();
        let output = widget.build(Ui::new(self, context, &scratch, root));
        scratch.prepare();
        self.scratch = scratch;
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
            let id = self.nodes[node.index()].widget_id;
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
        let nodes = &self.nodes;
        self.watch = self.laid_out.rebuild(nodes.len(), |index| nodes[index].widget_id);
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

    /// returns the area of the node `target` names from the root in the latest layout
    ///
    /// valid after [`Self::layout`]
    pub fn area(&self, target: impl Into<Target>) -> Option<Rect<C::Scalar>> {
        let id = ui::target_id(self, NodeId::new(0), target.into());
        self.laid_out
            .get(id, |index| self.nodes[index].widget_id)
            .map(|index| self.placements[index].area)
    }
}

//
// internals
//

impl<C: Context> Kernel<C> {
    #[track_caller]
    fn push_node(&mut self, context: &mut C, parent: Option<NodeId>, widget_id: Id) -> NodeId {
        if self.watch == Some(widget_id) {
            context.duplicate_id(widget_id, Location::caller());
        }
        let id = NodeId::new(self.nodes.len());
        self.nodes.push(StoredNode {
            widget_id,
            parent: parent.unwrap_or(id),
            subtree_end: id.index() as u32,
            first_measure: StoredMeasureId::NONE,
            last_paint: StoredPaintId::NONE,
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
                    clip: None,
                    limits: Constraints::loose(Size::uniform(C::Scalar::UNBOUNDED)),
                    padding: Sides::all(C::Scalar::ZERO),
                    hit: Sides::all(C::Scalar::ZERO),
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
