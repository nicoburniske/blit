use std::{
    hash::{Hash, Hasher},
    num::NonZeroU64,
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use crate::{geometry::Point, hash::FxHasher, macros::flags};

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WidgetId(NonZeroU64);

impl WidgetId {
    /// creates an absolute widget id from a stable value
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
    /// recognition thresholds in frame coordinates and coordinates per second
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct InteractionConfig {
        new(),
        drag_threshold: f32 = 6.0,
        long_press_delay: Duration = Duration::from_millis(500),
        double_tap_delay: Duration = Duration::from_millis(300),
        double_tap_distance: f32 = 12.0,
        swipe_distance: f32 = 30.0,
        swipe_speed: f32 = 250.0,
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
    pub double_tapped: bool,
    pub long_pressed: bool,
    pub drag: Option<Drag<T>>,
    pub scroll: Option<Scroll>,
    pub swipe: Option<Direction>,
    pub pinch: Option<Pinch<T>>,
    pub rotation: Option<Rotation<T>>,
}

impl<T> Default for Interaction<T> {
    fn default() -> Self {
        Self {
            active: false,
            activated: false,
            deactivated: false,
            hovered: false,
            clicked: false,
            double_tapped: false,
            long_pressed: false,
            drag: None,
            scroll: None,
            swipe: None,
            pinch: None,
            rotation: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag<T> {
    pub phase: Phase,
    pub position: Point<T>,
    /// movement since the previous input including normal completion
    pub delta: Point<T>,
    /// coordinate units per second
    pub velocity: Point<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scroll {
    pub phase: Phase,
    pub delta: Point<f32>,
    pub continuous: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pinch<T> {
    pub phase: Phase,
    pub center: Point<T>,
    /// scale multiplier since the previous input
    pub scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation<T> {
    pub phase: Phase,
    pub center: Point<T>,
    /// rotation in radians since the previous input
    pub radians: f32,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

flags! {
    pub struct Sense(pub u16) {
        NONE = 0,
        ALL = (1 << 12) - 1,
        CLICK = 1,
        DRAG_X = 1 << 1,
        DRAG_Y = 1 << 2,
        DRAG = Self::DRAG_X.0 | Self::DRAG_Y.0,
        CLICK_AND_DRAG = Self::CLICK.0 | Self::DRAG.0,
        FOCUS = Self::CLICK.0 | (1 << 3),
        SCROLL_X = 1 << 4,
        SCROLL_Y = 1 << 5,
        SCROLL = Self::SCROLL_X.0 | Self::SCROLL_Y.0,
        SCROLL_AND_DRAG = Self::SCROLL.0 | Self::DRAG.0,
        DOUBLE_TAP = 1 << 6,
        LONG_PRESS = 1 << 7,
        SWIPE_X = 1 << 8,
        SWIPE_Y = 1 << 9,
        SWIPE = Self::SWIPE_X.0 | Self::SWIPE_Y.0,
        PINCH = 1 << 10,
        ROTATE = 1 << 11,
    }
}
