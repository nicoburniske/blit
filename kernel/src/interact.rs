use std::{
    hash::{Hash, Hasher},
    num::{NonZeroU32, NonZeroU64},
    time::Duration,
};

use crate::{GestureCx, geometry::Point, internal::hash::FxHasher, macros::flags};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(NonZeroU64);

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

/// an owned snapshot of this widget's interaction
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Interaction {
    pub active: bool,
    pub activated: bool,
    pub deactivated: bool,
    pub hovered: bool,
    pub clicked: bool,
    pub scroll: Option<Scroll>,
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
    /// captured by the widget reading this contact
    pub captured: bool,
}

/// identifies one press or touch within a frame instance
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContactId(pub(crate) NonZeroU32);

/// kind of pointer producing a contact
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactSource {
    Mouse(crate::PointerButton),
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

flags! {
    pub struct Sense(pub u16) {
        NONE = 0,
        ALL = (1 << 4) - 1,
        CLICK = 1,
        FOCUS = Self::CLICK.0 | (1 << 1),
        SCROLL_X = 1 << 2,
        SCROLL_Y = 1 << 3,
        SCROLL = Self::SCROLL_X.0 | Self::SCROLL_Y.0,
    }
}

/// a recognizer the kernel retains for one widget
///
/// it updates before every build, deepest widget first, so it can capture contacts before its
/// ancestors and click see them
pub trait Gesture<T>: Copy + 'static {
    /// what the gesture recognized during one update, default before the first
    type Output: Copy + Default + 'static;

    fn update(&mut self, cx: &mut GestureCx<'_, T>) -> Self::Output;
}
