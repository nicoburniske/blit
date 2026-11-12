//! a small immediate-mode kernel for building user interfaces
//!
//! each frame, you describe the interface as a tree of nodes. blit calculates the size and
//! position of each node, then paints its contents
//!
//! # core model
//!
//! - a node is the retained unit of geometry and composition. it may hold properties, atoms, an
//!   optional layout, and child nodes
//! - [`Widget`] builds through a [`Ui`] whose mode controls its available operations. the default
//!   [`state::Build`] mode may establish a layout and build children. [`state::Node`] augments an
//!   existing node without changing its structure
//! - [`Atom`] is visual content retained for painting. it measures under constraints, then paints
//!   using the node's resolved position and size
//! - [`Layout`] is retained policy for a node's children. it measures them and resolves their
//!   positions and sizes
//!
//! # context
//!
//! the [`Context`] is usually your renderer. widgets, layouts and atoms all get mutable access to
//! it, whether to measure text, paint, or use anything else it provides
//!
//! - [`Paint`] is how atoms draw. each implementation accepts one kind of drawing operation in
//!   frame coordinates, and the context decides what painting means
//! - [`HasFrameStore`] provides access to a context's [`FrameStore`]. widgets reserve drawing inputs
//!   with [`FrameStore::slice`], fill the builder and finish it into a [`FrameSlice`]
//!   the context resolves this handle when rendering
//!
//! # frame lifecycle
//!
//! ```text
//! Frame::build
//!     │ records nodes, Layouts, and Atoms
//!     │ Ui::storage_mut provides frame storage for drawing inputs
//!     ▼
//! retained frame graph
//!     │ Frame::layout measures Atoms and arranges children
//!     ▼
//! resolved node positions and sizes
//!     │ Frame::paint invokes Atom::paint, which sends commands through Paint
//!     ▼
//! context
//!     │ renders its commands, then clears their stored inputs
//!     ▼
//! output
//! ```
//!
//! widgets therefore describe a frame now, while layouts and atoms do their work later in that
//! frame.
//!
//! the graph is rebuilt every frame
//!
//! # storage
//!
//! ```text
//! Scratch      Ui::scratch().slice(capacity), also on LayoutCx and AtomCx
//!              temporary slices for the current step, reclaimed when it ends
//!
//! FrameSlice   Ui::storage_mut().slice(capacity)
//!              drawing inputs owned by the context until it renders
//! ```

mod clip;
mod frame;
mod frame_store;
mod macros;
pub mod scratch;

pub mod animation;
pub mod geometry;
pub mod input;
pub mod interact;
#[doc(hidden)]
pub mod internal;
pub mod layout;

pub use self::{
    animation::{Animation, AnimationOptions, Easing, Motion, MotionProps, Spring, Tween},
    clip::{Clip, ClipShape},
    frame::{
        Frame, NodeId,
        atom::AtomCx,
        interact::{Contacts, InteractCx},
        ui::{NodeTarget, Ui, state},
    },
    frame_store::{FrameSlice, FrameSliceBuilder, FrameStore},
    geometry::{
        Constraints, LogicalPoint, LogicalRect, LogicalSize, PhysicalPoint, PhysicalRect, PhysicalSize, Point, Rect,
        Scalar, Scale2, Sides, Size,
    },
    input::{Input, Key, KeyInput, Modifiers, MouseButton, ScrollPhase, TouchPhase},
    interact::{Contact, ContactId, ContactSource, Interact, KeyboardEvent, Phase, ScrollEvent},
    layout::{Axis, Layout, LayoutCx},
    scratch::{Scratch, ScratchBuffer},
};

/// resources shared by every widget, layout and atom in a frame
pub trait Context {
    /// coordinate type for all frame geometry
    type Scalar: Scalar;
}

impl Context for () {
    type Scalar = f32;
}

/// accepts one kind of drawing operation in frame coordinates
pub trait Paint<T>: Context {
    /// area supplies placement and dimensions while the operation defines any outsets
    fn paint(&mut self, area: Rect<Self::Scalar>, command: T);
}

/// provides access to drawing inputs owned by a context
pub trait HasFrameStore {
    fn storage(&self) -> &FrameStore;
    fn storage_mut(&mut self) -> &mut FrameStore;
}

builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct FrameInfo<T> {
        new(size: Size<T>),
    }
}

/// immediate builder whose ui mode determines its available operations
pub trait Widget<C: Context, S = state::Build> {
    type Response;

    fn build(self, ui: Ui<'_, C, S>) -> Self::Response;
}

/// retained visual content that measures and paints after building
///
/// atoms can be passed directly to [`Ui::insert`]
pub trait Atom<C: Context>: 'static {
    /// returns the size requested by this atom under `constraints`
    ///
    /// measurement may be skipped under tight constraints. painting must not
    /// depend on prior measurement.
    fn measure(&self, cx: &mut AtomCx<'_, C>, constraints: Constraints<C::Scalar>) -> Size<C::Scalar>;

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: Rect<C::Scalar>);

    /// conservative bounds containing everything this atom may paint
    ///
    /// override when painting extends beyond the layout `area`
    fn paint_bounds(&self, area: Rect<C::Scalar>) -> Rect<C::Scalar> {
        area
    }
}

/// stable identity for widgets, animations and timers
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(std::num::NonZeroU64);

impl Id {
    /// creates an absolute id from a stable value
    pub fn new(source: impl std::hash::Hash) -> Self {
        use std::num::NonZeroU64;

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
        match std::num::NonZeroU64::new(hasher.finish()) {
            Some(value) => Self(value),
            None => Self(std::num::NonZeroU64::MIN),
        }
    }

    /// composes a stable child id without using the current ui node
    pub fn child(self, source: impl std::hash::Hash) -> Self {
        Self::new((self, source))
    }
}

/// converts atoms and node widgets for insertion
///
/// implemented automatically for atoms and node widgets. [`Ui::insert`] infers the output type.
/// types implementing both require an explicit output widget type.
pub trait IntoWidget<C: Context, W> {
    fn into_widget(self) -> W;
}

/// adapts an atom to a node widget
pub struct AtomWidget<A>(A);

impl<C: Context, W: Widget<C, state::Node>> IntoWidget<C, W> for W {
    fn into_widget(self) -> W {
        self
    }
}

impl<C: Context, A: Atom<C>> IntoWidget<C, AtomWidget<A>> for A {
    fn into_widget(self) -> AtomWidget<A> {
        AtomWidget(self)
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

impl<C: Context> Atom<C> for () {
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<C::Scalar>) -> Size<C::Scalar> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, _: &mut AtomCx<'_, C>, _: Rect<C::Scalar>) {}

    fn paint_bounds(&self, _: Rect<C::Scalar>) -> Rect<C::Scalar> {
        Rect::default()
    }
}

#[cfg(doctest)]
#[doc = include_str!("../tests/compile_fail.md")]
mod compile_fail {}
