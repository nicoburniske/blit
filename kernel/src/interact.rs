use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{geometry::Point, input::ScrollPhase};

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WidgetId(u64);

impl WidgetId {
    pub fn new(source: impl Hash) -> Self {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        Self(hasher.finish())
    }

    pub fn unique() -> Self {
        Self::new(("blit widget", NEXT_ID.fetch_add(1, Ordering::Relaxed)))
    }

    pub fn child(self, source: impl Hash) -> Self {
        Self::new((self, source))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sense {
    pub click: bool,
    pub drag: bool,
    pub focus: bool,
    pub scroll: bool,
}

impl Sense {
    pub const CLICK: Self = Self {
        click: true,
        drag: false,
        focus: false,
        scroll: false,
    };
    pub const CLICK_AND_DRAG: Self = Self {
        click: true,
        drag: true,
        focus: false,
        scroll: false,
    };
    pub const DRAG: Self = Self {
        click: false,
        drag: true,
        focus: false,
        scroll: false,
    };
    pub const FOCUS: Self = Self {
        click: true,
        drag: false,
        focus: true,
        scroll: false,
    };
    pub const SCROLL: Self = Self {
        click: false,
        drag: false,
        focus: false,
        scroll: true,
    };
    pub const SCROLL_AND_DRAG: Self = Self {
        click: false,
        drag: true,
        focus: false,
        scroll: true,
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Interaction {
    pub active: bool,
    pub activated: bool,
    pub deactivated: bool,
    pub hovered: bool,
    pub clicked: bool,
    pub dragging: bool,
    pub drag_delta: Point,
    pub scroll: Option<ScrollInteraction>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollInteraction {
    pub delta: Point,
    pub continuous: bool,
    pub phase: ScrollPhase,
}
