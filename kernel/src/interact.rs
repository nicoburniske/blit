use std::{num::NonZeroU32, time::Duration};

use crate::{Axis, InteractCx, Modifiers, PointerButton, Scalar, geometry::Point};

/// an interaction kind the kernel retains for one widget
///
/// it updates before every build, deepest widget first, so it can claim contacts before its ancestors see them
/// kinds on one widget update in reverse order of their first requests in the previous build
/// the first update runs on registration without resolved geometry, routed input, or changes to input ownership
pub trait Interact<T>: Copy + 'static {
    /// what the kind recognized during one update
    type Response: Copy + 'static;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> Self::Response;
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
    Touch,
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

/// clicks and press feedback
#[derive(Clone, Copy, Debug)]
pub struct Press {
    slop: f32,
    contact: Option<ContactId>,
    active: bool,
}

impl Press {
    pub const fn new() -> Self {
        Self {
            slop: 6.0,
            contact: None,
            active: false,
        }
    }

    /// movement in frame coordinates after which a press is no longer a click
    pub const fn slop(self, slop: f32) -> Self {
        Self { slop, ..self }
    }
}

impl Default for Press {
    fn default() -> Self {
        Self::new()
    }
}

/// the response of [`Press`]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PressResponse {
    pub active: bool,
    pub activated: bool,
    pub deactivated: bool,
    pub hovered: bool,
    pub clicked: bool,
}

impl<T: Scalar> Interact<T> for Press {
    type Response = PressResponse;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> PressResponse {
        let mut active = false;
        let mut clicked = false;
        // claims last one update, so keep claiming while it is still a click
        if let Some(id) = self.contact
            && let Some(contact) = cx.contacts().find(|contact| contact.id == id)
        {
            let x = (contact.position.x - contact.origin.x).to_f32();
            let y = (contact.position.y - contact.origin.y).to_f32();
            // this widget capturing it, like with its own drag, ends the click
            if x * x + y * y < self.slop * self.slop && cx.claim(id) {
                active = !matches!(contact.phase, Phase::Ended | Phase::Cancelled);
                clicked = contact.phase == Phase::Ended && contact.over;
            }
        }
        // one press at a time, deeper widgets claimed theirs first
        if !active {
            self.contact = cx
                .contacts()
                .find(|contact| {
                    contact.phase == Phase::Started
                        && contact.free
                        && matches!(
                            contact.source,
                            ContactSource::Touch | ContactSource::Mouse(PointerButton::Primary)
                        )
                })
                .map(|contact| contact.id)
                .filter(|id| cx.claim(*id));
            active = self.contact.is_some();
        }
        let state = PressResponse {
            active,
            activated: active && !self.active,
            deactivated: self.active && !active,
            hovered: cx.hovered(),
            clicked,
        };
        self.active = active;
        state
    }
}

/// takes keyboard focus when pressed or tapped, returning whether this widget is focused
#[derive(Clone, Copy, Debug, Default)]
pub struct Focus;

impl<T: Scalar> Interact<T> for Focus {
    type Response = bool;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> bool {
        if cx.contacts().any(|contact| {
            contact.free
                && match contact.source {
                    ContactSource::Mouse(PointerButton::Primary) => contact.phase == Phase::Started,
                    ContactSource::Touch => contact.phase == Phase::Ended,
                    ContactSource::Mouse(_) => false,
                }
        }) {
            cx.focus();
        }
        cx.focused()
    }
}

/// wheel and trackpad scrolling, keeping continuous sequences until they end
#[derive(Clone, Copy, Debug, Default)]
pub struct Scroll {
    axis: Option<Axis>,
    active: bool,
}

impl Scroll {
    pub const fn new() -> Self {
        Self {
            axis: None,
            active: false,
        }
    }

    /// takes only scrolling that starts mostly along `axis`, leaving the rest to ancestors
    pub const fn axis(self, axis: Axis) -> Self {
        Self {
            axis: Some(axis),
            ..self
        }
    }
}

/// the response of [`Scroll`]
///
/// `activated` implies `active` unless `deactivated`, and `released` implies `deactivated`
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollResponse {
    /// this widget holds a continuous trackpad sequence
    pub active: bool,
    pub activated: bool,
    pub deactivated: bool,
    /// the held sequence ended by lifting rather than being cancelled
    pub released: bool,
    /// scrolling during this update
    pub delta: Point<f32>,
    /// trackpad scrolling rather than wheel steps
    pub continuous: bool,
}

impl<T: Scalar> Interact<T> for Scroll {
    type Response = ScrollResponse;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> ScrollResponse {
        let held = self.active;
        let Some(event) = cx.scroll() else {
            return ScrollResponse {
                active: held,
                continuous: held,
                ..ScrollResponse::default()
            };
        };
        let Point { x, y } = event.delta;
        // a held sequence stays whatever its direction, so only one not yet held waits for a direction
        if !held
            && ((x == 0.0 && y == 0.0)
                || self
                    .axis
                    .is_some_and(|axis| (axis == Axis::Horizontal) != (x.abs() > y.abs())))
        {
            return ScrollResponse::default();
        }
        // a cancellation ends this widget's sequence, so it must not take the next one
        if event.phase != Phase::Cancelled {
            cx.capture_scroll();
        }
        self.active = event.continuous && matches!(event.phase, Phase::Started | Phase::Changed);
        ScrollResponse {
            active: self.active,
            activated: self.active && !held,
            deactivated: held && !self.active,
            released: held && event.phase == Phase::Ended,
            delta: event.delta,
            continuous: event.continuous,
        }
    }
}
