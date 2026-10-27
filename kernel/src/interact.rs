use std::{
    hash::{Hash, Hasher},
    num::{NonZeroU32, NonZeroU64},
    time::Duration,
};

use crate::{Axis, InteractCx, PointerButton, Scalar, geometry::Point, internal::hash::FxHasher};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(pub(crate) NonZeroU64);

impl Id {
    /// creates an absolute id from a stable value
    pub fn new(source: impl Hash) -> Self {
        let mut hasher = FxHasher::default();
        source.hash(&mut hasher);
        Self(NonZeroU64::new(hasher.finish()).unwrap_or(NonZeroU64::MIN))
    }

    /// composes a stable child id without using the current ui node
    pub fn child(self, source: impl Hash) -> Self {
        Self::new((self, source))
    }
}

/// an interaction kind the kernel retains for one widget
///
/// it updates before every build, deepest widget first, so it can claim contacts before its ancestors see them
/// the first update runs on registration with the previous layout and no routed input or changes to input ownership
pub trait Interact<T>: Copy + 'static {
    /// what the kind recognized during one update
    type Output: Copy + 'static;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> Self::Output;
}

/// a mouse press or touch routed through this widget
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
    pub modifiers: crate::Modifiers,
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scroll {
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

/// the output of [`Press`]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PressState {
    pub active: bool,
    pub activated: bool,
    pub deactivated: bool,
    pub hovered: bool,
    pub clicked: bool,
}

impl<T: Scalar> Interact<T> for Press {
    type Output = PressState;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> PressState {
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
                clicked = contact.phase == Phase::Ended && cx.over(contact.position);
            }
        }
        // one press at a time, deeper widgets claimed theirs first
        if !active {
            self.contact = cx
                .contacts()
                .find(|contact| {
                    contact.phase == Phase::Started
                        && matches!(
                            contact.source,
                            ContactSource::Touch | ContactSource::Mouse(PointerButton::Primary)
                        )
                })
                .map(|contact| contact.id)
                .filter(|id| cx.claim(*id));
            active = self.contact.is_some();
        }
        let state = PressState {
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
    type Output = bool;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> bool {
        if cx.contacts().any(|contact| match contact.source {
            ContactSource::Mouse(PointerButton::Primary) => contact.phase == Phase::Started,
            ContactSource::Touch => contact.phase == Phase::Ended,
            ContactSource::Mouse(_) => false,
        }) {
            cx.focus();
        }
        cx.focused()
    }
}

/// wheel and trackpad scrolling, keeping continuous sequences until they end
#[derive(Clone, Copy, Debug, Default)]
pub struct Wheel {
    axis: Option<Axis>,
}

impl Wheel {
    pub const fn new() -> Self {
        Self { axis: None }
    }

    /// takes only scrolling that starts mostly along `axis`, leaving the rest to ancestors
    pub const fn axis(self, axis: Axis) -> Self {
        Self { axis: Some(axis) }
    }
}

impl<T: Scalar> Interact<T> for Wheel {
    type Output = Option<Scroll>;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> Option<Scroll> {
        let scroll = cx.wheel()?;
        let Point { x, y } = scroll.delta;
        // a held sequence stays whatever its direction, so only its start is checked
        let starts = scroll.phase == Phase::Started || !scroll.continuous;
        if starts
            && (x != 0.0 || y != 0.0)
            && self
                .axis
                .is_some_and(|axis| (axis == Axis::Horizontal) != (x.abs() > y.abs()))
        {
            return None;
        }
        cx.capture_wheel();
        Some(scroll)
    }
}
