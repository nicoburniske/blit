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
//! # frame lifecycle
//!
//! ```text
//! Frame::build
//!     │ records nodes, Layouts, and Atoms
//!     ▼
//! retained frame graph
//!     │ Frame::layout measures Atoms and arranges children
//!     ▼
//! resolved node positions and sizes
//!     │ Frame::paint invokes Atom::paint
//!     ▼
//! output
//! ```
//!
//! widgets therefore describe a frame now, while layouts and atoms do their work later in that
//! frame.
//!
//! the graph is rebuilt every frame

mod clip;
mod frame;
mod macros;

pub mod animation;
pub mod geometry;
pub mod input;
pub mod interact;
#[doc(hidden)]
pub mod internal;
pub mod layout;

pub use self::{
    animation::{Easing, Transition, TransitionProperties},
    clip::{Clip, ClipShape},
    frame::{
        Frame, InteractionScope, NodeId, NodeTarget, Ui,
        atom::AtomCx,
        bump::FrameSlice,
        interaction::{Contacts, GestureCx},
        state,
    },
    geometry::{
        Constraints, LogicalPoint, LogicalRect, LogicalSize, PhysicalPoint, PhysicalRect, PhysicalSize, Point, Rect,
        Scalar, Scale2, Sides, Size,
    },
    input::{Input, Key, KeyInput, Modifiers, PointerButton, ScrollPhase, TouchPhase},
    interact::{Contact, ContactId, ContactSource, Gesture, Id, Interaction, Phase, Scroll, Sense},
    layout::{Axis, Layout, LayoutCx, Scratch, Sizing},
};

pub trait Context {
    type Scalar: Scalar;
}

impl Context for () {
    type Scalar = f32;
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
