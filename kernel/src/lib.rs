//! a small immediate-mode kernel for building platform-specific user interfaces
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
//! - [`Content`] works within an existing node. it may configure the node, use frame and platform
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
//! platform output
//! ```
//!
//! widgets therefore describe a frame now, while layouts and atoms do their work later in that
//! frame.
//!
//! the graph is rebuilt every frame

mod arena;
mod frame;
mod macros;

pub mod animation;
pub mod geometry;
pub mod input;
pub mod interact;
pub mod layout;

pub use animation::{Easing, Transition, TransitionProperties};
pub use frame::{
    Absolute, Anchor, Frame, FrameMemory, LayerId, NodeId, Place, PlaceKind, PositionTarget,
    Sizing, Ui, state,
};
pub use geometry::{
    Constraints, LogicalPoint, LogicalRect, LogicalSize, PhysicalPoint, PhysicalRect, PhysicalSize,
    Point, Rect, Scale2, Sides, Size,
};
pub use input::{Input, Key, KeyInput, Modifiers, PointerButton, ScrollPhase};
pub use interact::{Interaction, ScrollInteraction, Sense, WidgetId};
pub use layout::{Axis, Children, Layout, LayoutCx, LayoutResolution};

pub trait Platform {
    fn begin(&mut self, frame: FrameInfo);

    fn end(&mut self);

    fn interaction_area(&self, area: Rect, clip: Rect) -> Option<Rect> {
        area.intersection(clip)
    }
}

crate::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct FrameInfo {
        new(size: Size),
        layout_resolution: LayoutResolution = LayoutResolution::Continuous,
    }
}

/// immediate builder that owns and populates a new frame node
pub trait Widget<R: Platform> {
    type Response;

    fn build(self, ui: Ui<'_, R>) -> Self::Response;
}

/// immediate content that augments an existing node without changing its structure
pub trait Content<R: Platform> {
    type Response;

    fn append(self, ui: Ui<'_, R, state::Node>) -> Self::Response;
}

/// retained visual content that measures and paints after building
///
/// every atom implements [`Content`]
pub trait Atom<R: Platform>: 'static {
    fn measure(&self, platform: &mut R, constraints: Constraints) -> Size;

    fn paint(&self, platform: &mut R, area: Rect);

    /// whether measurement must be repeated when constraints tighten
    fn measure_depends_on_constraints(&self) -> bool {
        true
    }
}

impl<R, F, O> Widget<R> for F
where
    R: Platform,
    F: FnOnce(Ui<'_, R>) -> O,
{
    type Response = O;

    fn build(self, ui: Ui<'_, R>) -> Self::Response {
        self(ui)
    }
}

impl<R: Platform> Widget<R> for () {
    type Response = ();

    fn build(self, mut ui: Ui<'_, R>) {
        ui.insert(self);
    }
}

impl<R: Platform> Atom<R> for () {
    fn measure(&self, _: &mut R, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, _: &mut R, _: Rect) {}

    fn measure_depends_on_constraints(&self) -> bool {
        false
    }
}

pub trait Clip<R: Platform>: Copy + 'static {
    fn push(&self, platform: &mut R, area: Rect);

    fn pop(&self, platform: &mut R);
}

#[cfg(doctest)]
#[doc = include_str!("../tests/compile_fail.md")]
mod compile_fail {}
