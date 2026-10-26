use std::{
    hash::{Hash, Hasher},
    num::{NonZeroU32, NonZeroU64},
    time::Duration,
};

use crate::{Input, InteractCx, PointerButton, Scalar, geometry::Point, internal::hash::FxHasher};

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
/// it updates before every build, deepest widget first, so it can claim contacts before its
/// ancestors see them
pub trait Interact<T>: Copy + 'static {
    /// what the kind recognized during one update, default before the first
    type Output: Copy + Default + 'static;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> Self::Output;
}

/// a mouse press or touch routed through this widget
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact<T> {
    pub id: ContactId,
    pub source: ContactSource,
    pub phase: Phase,
    pub hold: Hold,
    pub origin: Point<T>,
    pub position: Point<T>,
    /// movement since the previous input
    pub delta: Point<T>,
    /// frame clock time of the press
    pub started: Duration,
    pub modifiers: crate::Modifiers,
    /// still within the tap slop of its origin
    pub tap: bool,
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

/// who holds a contact, as seen by the widget reading it
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    Free,
    /// tentatively held by this widget until something captures it
    Claimed,
    ClaimedElsewhere,
    Captured,
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
#[derive(Clone, Copy, Debug, Default)]
pub struct Press {
    active: bool,
}

/// the output of [`Press`]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Interaction {
    pub active: bool,
    pub activated: bool,
    pub deactivated: bool,
    pub hovered: bool,
    pub clicked: bool,
}

impl Press {
    pub const fn new() -> Self {
        Self { active: false }
    }
}

impl<T: Scalar> Interact<T> for Press {
    type Output = Interaction;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> Interaction {
        let mut active = false;
        let mut clicked = false;
        for contact in cx.contacts() {
            let live = !matches!(contact.phase, Phase::Ended | Phase::Cancelled);
            match contact.hold {
                Hold::Claimed if contact.tap => {
                    active |= live;
                    clicked |= contact.phase == Phase::Ended && cx.over(contact.position);
                }
                Hold::Captured => active |= live,
                _ => {}
            }
        }
        // one press at a time, deeper widgets claimed theirs first
        if !active
            && let Some(contact) = cx.contacts().find(|contact| {
                contact.hold == Hold::Free
                    && contact.phase == Phase::Started
                    && matches!(
                        contact.source,
                        ContactSource::Touch | ContactSource::Mouse(PointerButton::Primary)
                    )
            })
        {
            cx.claim(contact.id);
            active = true;
        }
        let interaction = Interaction {
            active,
            activated: active && !self.active,
            deactivated: self.active && !active,
            hovered: cx.hovered(),
            clicked,
        };
        self.active = active;
        interaction
    }
}

/// takes keyboard focus when pressed or tapped, returning whether this widget is focused
#[derive(Clone, Copy, Debug, Default)]
pub struct Focus;

impl<T: Scalar> Interact<T> for Focus {
    type Output = bool;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> bool {
        if cx.contacts().any(|contact| {
            contact.hold != Hold::ClaimedElsewhere
                && match contact.source {
                    ContactSource::Mouse(PointerButton::Primary) => contact.phase == Phase::Started,
                    ContactSource::Touch => contact.phase == Phase::Ended && contact.tap,
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
pub struct Wheel;

impl<T: Scalar> Interact<T> for Wheel {
    type Output = Option<Scroll>;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> Option<Scroll> {
        let scroll = cx.wheel()?;
        cx.capture_wheel();
        Some(scroll)
    }
}

/// the current input when it routes through this widget
#[derive(Clone, Copy, Debug, Default)]
pub struct Raw;

impl<T: Scalar> Interact<T> for Raw {
    type Output = Input<T>;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> Input<T> {
        cx.input()
    }
}
