use std::{num::NonZeroU32, time::Duration};

use crate::{Context, InteractCx, Modifiers, PointerButton, geometry::Point};

/// how a widget recognizes input, configured by its latest build
///
/// updates before every build, deepest widget first, so deeper widgets claim contacts before their ancestors
pub trait Interact<C: Context>: Copy + 'static {
    /// kept across builds
    type State: Copy + Default + 'static;
    /// what one update recognized
    type Response: Copy + 'static;

    fn update(&self, state: &mut Self::State, cx: &mut InteractCx<'_, C::Scalar>) -> Self::Response;
}

/// a mouse press or touch, as seen by one widget
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact<T> {
    pub id: ContactId,
    pub source: ContactSource,
    pub phase: Phase,
    pub origin: Point<T>,
    pub position: Point<T>,
    /// movement since the previous input
    pub delta: Point<T>,
    /// frame clock time of the press
    pub started: Duration,
    pub modifiers: Modifiers,
    /// the position falls on this widget rather than something above it
    pub over: bool,
    /// free for this widget to use
    pub free: bool,
}

/// identifies one press or touch within a frame instance
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContactId(pub(crate) NonZeroU32);

/// kind of pointer producing a contact
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactSource {
    Mouse(PointerButton),
    Touch(u64),
}

/// wheel or trackpad scrolling routed through this widget
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollEvent {
    pub phase: Phase,
    pub delta: Point<f32>,
    pub continuous: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    /// no sample received during this build
    Held = 0,
    Started = 1,
    Changed = 2,
    Ended = 3,
    /// aborted without committing the action
    Cancelled = 4,
}
