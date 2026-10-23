use std::{
    hash::{Hash, Hasher},
    num::NonZeroU64,
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use crate::{geometry::Point, internal::hash::FxHasher, macros::flags};

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(NonZeroU64);

impl Id {
    /// creates an absolute id from a stable value
    pub fn new(source: impl Hash) -> Self {
        let mut hasher = FxHasher::default();
        source.hash(&mut hasher);
        Self(NonZeroU64::new(hasher.finish()).unwrap_or(NonZeroU64::MIN))
    }

    pub fn unique() -> Self {
        Self::new(("blit widget", NEXT_ID.fetch_add(1, Ordering::Relaxed)))
    }

    /// composes a stable child id without using the current ui node
    pub fn child(self, source: impl Hash) -> Self {
        Self::new((self, source))
    }
}

crate::builder! {
    /// pointer routing thresholds in frame coordinates
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct InteractionConfig {
        new(),
        drag_threshold: f32 = 6.0,
    }
}

/// an owned snapshot of this widget's interaction
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interaction<T> {
    pub active: bool,
    pub activated: bool,
    pub deactivated: bool,
    pub hovered: bool,
    pub clicked: bool,
    pub drag: Option<Drag<T>>,
    pub scroll: Option<Scroll>,
}

impl<T> Default for Interaction<T> {
    fn default() -> Self {
        Self {
            active: false,
            activated: false,
            deactivated: false,
            hovered: false,
            clicked: false,
            drag: None,
            scroll: None,
        }
    }
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
    /// frame clock time of the last input for this contact
    pub updated: Duration,
    pub modifiers: crate::Modifiers,
    /// captured by the widget reading this contact
    pub captured: bool,
}

/// identifies one press or touch within a frame instance
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContactId(pub(crate) u64);

/// platform identity which may be reused after release
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactSource {
    Mouse(crate::PointerButton),
    Touch(u64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag<T> {
    pub phase: Phase,
    pub position: Point<T>,
    /// movement since the previous input including normal completion
    pub delta: Point<T>,
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
    Started = 1,
    Changed = 2,
    Ended = 3,
    /// aborted without committing the action
    Cancelled = 4,
}

flags! {
    pub struct Sense(pub u16) {
        NONE = 0,
        ALL = (1 << 6) - 1,
        CLICK = 1,
        DRAG_X = 1 << 1,
        DRAG_Y = 1 << 2,
        DRAG = Self::DRAG_X.0 | Self::DRAG_Y.0,
        FOCUS = Self::CLICK.0 | (1 << 3),
        SCROLL_X = 1 << 4,
        SCROLL_Y = 1 << 5,
        SCROLL = Self::SCROLL_X.0 | Self::SCROLL_Y.0,
    }
}
