//! a small immediate-mode kernel for building platform-specific user interfaces
//!
//! each frame, you describe the interface as a tree of nodes. blit calculates the size and
//! position of each node, then paints its contents
//!
//! # core model
//!
//! - a node is the retained unit of geometry and composition. it may hold properties, zero or more
//!   atoms, an optional layout, and zero or more child nodes. children require a layout
//! - a [`Widget`] is consumed while building. it records content into one [`Ui`] node and may
//!   return an immediate response, but is never measured or painted
//! - an [`Atom`] is owned visual content retained for the later phases of the frame. it measures
//!   under constraints, then paints with its node's resolved rectangle
//! - a [`Layout`] is retained spatial policy. it measures and positions the node's children
//!
//! # building nodes
//!
//! [`Ui`] tracks node construction with three states:
//!
//! - [`state::Build`] is a node without a layout. it can receive atoms or establish its layout
//! - [`state::Open`] has a layout. it can receive atoms and create children
//! - [`state::Pending`] is a new child waiting for a widget or its own layout
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
//! resolved rectangles
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
    Absolute, Anchor, Frame, FrameMemory, LayerId, NodeId, Place, PositionTarget, Sizing, Ui, state,
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

/// content that can build into a [`Ui`] state
pub trait Widget<R: Platform, S = state::Build> {
    type Response;

    fn build(self, ui: Ui<'_, R, S>) -> Self::Response;
}

impl<R, S, F, O> Widget<R, S> for F
where
    R: Platform,
    F: FnOnce(Ui<'_, R, S>) -> O,
{
    type Response = O;

    fn build(self, ui: Ui<'_, R, S>) -> Self::Response {
        self(ui)
    }
}

impl<R: Platform, S> Widget<R, S> for () {
    type Response = ();

    fn build(self, mut ui: Ui<'_, R, S>) {
        ui.atom(self);
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

pub trait Atom<R: Platform>: Widget<R> + 'static {
    fn measure(&self, platform: &mut R, constraints: Constraints) -> Size;

    fn paint(&self, platform: &mut R, area: Rect);

    /// whether measurement must be repeated when constraints tighten
    fn measure_depends_on_constraints(&self) -> bool {
        true
    }
}

pub trait Clip<R: Platform>: Copy + 'static {
    fn push(&self, platform: &mut R, area: Rect);

    fn pop(&self, platform: &mut R);
}
