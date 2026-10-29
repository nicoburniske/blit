use std::{num::NonZeroU32, time::Duration};

use crate::{Axis, Context, InteractCx, Modifiers, PointerButton, Scalar, builder, geometry::Point};

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

builder! {
    /// clicks and press feedback
    ///
    /// a press that moves `slop` in frame coordinates is no longer a click
    #[derive(Clone, Copy, Debug)]
    pub struct Press {
        new(),
        slop: f32 = 6.0,
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

impl<C: Context> Interact<C> for Press {
    /// the press in progress
    type State = Option<ContactId>;
    type Response = PressResponse;

    fn update(&self, press: &mut Option<ContactId>, cx: &mut InteractCx<'_, C::Scalar>) -> PressResponse {
        let was_active = press.is_some();
        let mut active = false;
        let mut clicked = false;
        // claims last one update, so keep claiming while it is still a click
        if let Some(id) = *press
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
            *press = cx
                .contacts()
                .find(|contact| {
                    contact.phase == Phase::Started
                        && contact.free
                        && matches!(
                            contact.source,
                            ContactSource::Touch(_) | ContactSource::Mouse(PointerButton::Primary)
                        )
                })
                .map(|contact| contact.id)
                .filter(|id| cx.claim(*id));
            active = press.is_some();
        }
        PressResponse {
            active,
            activated: active && !was_active,
            deactivated: was_active && !active,
            hovered: cx.hovered(),
            clicked,
        }
    }
}

/// takes focus when pressed or tapped, returning whether this widget is focused
#[derive(Clone, Copy, Debug, Default)]
pub struct Focus;

impl<C: Context> Interact<C> for Focus {
    type State = ();
    type Response = bool;

    fn update(&self, _: &mut (), cx: &mut InteractCx<'_, C::Scalar>) -> bool {
        if cx.contacts().any(|contact| {
            contact.free
                && match contact.source {
                    ContactSource::Mouse(PointerButton::Primary) => contact.phase == Phase::Started,
                    ContactSource::Touch(_) => contact.phase == Phase::Ended,
                    ContactSource::Mouse(_) => false,
                }
        }) {
            cx.focus();
        }
        cx.focused()
    }
}

builder! {
    /// wheel and trackpad scrolling, keeping continuous sequences until they end
    ///
    /// with an `axis`, takes only scrolling that starts mostly along it, leaving the rest to ancestors
    #[derive(Clone, Copy, Debug)]
    pub struct Scroll {
        new(),
        #[into]
        axis: Option<Axis> = None,
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

impl<C: Context> Interact<C> for Scroll {
    /// holding a continuous sequence
    type State = bool;
    type Response = ScrollResponse;

    fn update(&self, active: &mut bool, cx: &mut InteractCx<'_, C::Scalar>) -> ScrollResponse {
        let held = *active;
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
        *active = event.continuous && matches!(event.phase, Phase::Started | Phase::Changed);
        ScrollResponse {
            active: *active,
            activated: *active && !held,
            deactivated: held && !*active,
            released: held && event.phase == Phase::Ended,
            delta: event.delta,
            continuous: event.continuous,
        }
    }
}
