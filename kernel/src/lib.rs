//! a small immediate-mode kernel for building user interfaces
//!
//! each frame, you describe the interface as a tree of nodes. blit calculates the size and
//! position of each node, then paints its contents
//!
//! # core model
//!
//! - a node is the retained unit of geometry and composition. it may hold properties, atoms, an
//!   optional layout, and child nodes
//! - [`Widget`] receives a fresh node and builds it. it may insert content or establish a layout
//!   and build children
//! - [`Content`] works within an existing node. it may configure the node, use frame and context
//!   services, and insert further content
//! - [`Atom`] is visual content retained for painting. it measures under constraints, then paints
//!   using the node's resolved position and size
//! - [`Layout`] is retained policy for a node's children. it measures them and resolves their
//!   positions and sizes
//!
//! # frame lifecycle
//!
//! ```text
//! Widget::build
//!     │ records nodes, Layouts, and Atoms
//!     ▼
//! retained frame graph
//!     │ Layout measures Atoms and arranges children
//!     ▼
//! resolved node positions and sizes
//!     │ Atom::paint
//!     ▼
//! output
//! ```
//!
//! widgets therefore describe a frame now, while layouts and atoms do their work later in that
//! frame.
//!
//! the graph is rebuilt every frame

mod arena;
mod frame;
mod hash;
mod macros;

pub mod animation;
pub mod geometry;
pub mod input;
pub mod interact;
pub mod layout;

pub use animation::{Easing, Transition, TransitionProperties};
pub use frame::{Absolute, Anchor, Frame, FrameMemory, NodeId, NodeTarget, Ui, state};
pub use geometry::{
    Constraints, LogicalPoint, LogicalRect, LogicalSize, PhysicalPoint, PhysicalRect, PhysicalSize,
    Point, Rect, Scale2, Sides, Size,
};
pub use input::{Input, Key, KeyInput, Modifiers, PointerButton, ScrollPhase};
pub use interact::{Interaction, ScrollInteraction, Sense, WidgetId};
pub use layout::{Axis, Layout, LayoutCx, LayoutResolution, Sizing};

/// state and services available while a frame is built, laid out and painted
pub trait Context {
    /// build and layout may repeat for queued inputs before paint and complete
    fn frame_stage(&mut self, _: FrameStage) {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameStage {
    Build,
    Layout,
    Paint,
    Complete,
}

crate::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct FrameInfo {
        new(size: Size),
        layout_resolution: LayoutResolution = LayoutResolution::Continuous,
    }
}

/// immediate builder that owns and populates a new frame node
pub trait Widget<C: Context> {
    type Response;

    fn build(self, ui: Ui<'_, C>) -> Self::Response;
}

/// immediate content that augments an existing node without changing its structure
pub trait Content<C: Context> {
    type Response;

    fn append(self, ui: Ui<'_, C, state::Node>) -> Self::Response;
}

/// retained visual content that measures and paints after building
///
/// every atom implements [`Content`]
pub trait Atom<C: Context>: 'static {
    /// returns the size requested by this atom under `constraints`
    ///
    /// measurement may be skipped under tight constraints. painting must not
    /// depend on prior measurement.
    fn measure(&self, context: &mut C, constraints: Constraints) -> Size;

    fn paint(&self, context: &mut C, area: Rect);

    /// conservative bounds containing everything this atom may paint
    ///
    /// these bounds may extend beyond the layout `area`
    fn paint_bounds(&self, area: Rect) -> Rect;
}

impl<C, F, O> Widget<C> for F
where
    C: Context,
    F: FnOnce(Ui<'_, C>) -> O,
{
    type Response = O;

    fn build(self, ui: Ui<'_, C>) -> Self::Response {
        self(ui)
    }
}

impl<C: Context> Widget<C> for () {
    type Response = ();

    fn build(self, mut ui: Ui<'_, C>) {
        ui.insert(self);
    }
}

impl<C: Context> Atom<C> for () {
    fn measure(&self, _: &mut C, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, _: &mut C, _: Rect) {}

    fn paint_bounds(&self, _: Rect) -> Rect {
        Rect::default()
    }
}

pub trait Clip<C: Context>: 'static {
    fn push(&self, context: &mut C, area: Rect);

    fn pop(&self, context: &mut C);
}

#[cfg(doctest)]
#[doc = include_str!("../tests/compile_fail.md")]
mod compile_fail {}
