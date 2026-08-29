use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{
    geometry::{LogicalPoint, LogicalRect},
    input::{Input, PointerButton, ScrollPhase},
};

static NEXT_ID: AtomicU32 = AtomicU32::new(1);
const DRAG_THRESHOLD: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WidgetId(u64);

impl WidgetId {
    #[inline]
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
    click: bool,
    drag: bool,
    focus: bool,
    scroll: bool,
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

/// primary-pointer ownership begins with `activated`, remains `active`, and ends with `deactivated`
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Interaction {
    /// owns primary pointer
    pub active: bool,
    /// ownership began on current input
    pub activated: bool,
    /// ownership ended on current input
    pub deactivated: bool,
    /// pointer inside hit area
    pub hovered: bool,
    /// primary pointer released inside hit area without dragging
    pub clicked: bool,
    /// owns drag beyond movement threshold while pointer is down
    pub dragging: bool,
    /// pointer movement since previous input during a drag, otherwise zero
    pub drag_delta: LogicalPoint,
    /// scroll input routed to this hit area
    pub scroll: Option<ScrollInteraction>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollInteraction {
    /// distance in logical pixels
    pub delta: LogicalPoint,
    /// pixel-based gesture rather than a discrete wheel step
    pub continuous: bool,
    /// gesture phase
    pub phase: ScrollPhase,
}

#[derive(Default)]
pub(crate) struct InteractionState {
    active: Option<WidgetId>,
    focused: Option<WidgetId>,
    hovered: Option<WidgetId>,
    drag_owner: Option<WidgetId>,
    scroll_owner: Option<WidgetId>,
    activated: Option<WidgetId>,
    deactivated: Option<WidgetId>,
    pointer: PointerState,
    previous_hits: Vec<HitItem>,
    current_hits: Vec<HitItem>,
    requests: Vec<(WidgetId, Sense)>,
    #[cfg(debug_assertions)]
    seen: std::collections::HashSet<WidgetId>,
}

#[derive(Default)]
struct PointerState {
    origin: LogicalPoint,
    position: Option<LogicalPoint>,
    down: bool,
    dragging: bool,
    event: PointerEvent,
}

#[derive(Clone, Copy, Default)]
enum PointerEvent {
    #[default]
    None,
    Down,
    Move(LogicalPoint),
    Up {
        leave: bool,
    },
    Scroll {
        delta: LogicalPoint,
        continuous: bool,
        phase: ScrollPhase,
    },
}

#[derive(Clone, Copy)]
struct HitItem {
    id: WidgetId,
    area: LogicalRect,
    sense: Sense,
}

impl InteractionState {
    pub fn begin_frame(&mut self, input: &Input) {
        #[cfg(debug_assertions)]
        self.seen.clear();
        self.requests.clear();

        self.pointer.event = PointerEvent::None;
        self.activated = None;
        self.deactivated = None;

        match *input {
            Input::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                self.pointer.origin = position;
                self.pointer.position = Some(position);
                self.pointer.down = true;
                self.pointer.dragging = false;
                self.pointer.event = PointerEvent::Down;
            }
            Input::PointerDown { position, .. } => self.pointer.position = Some(position),
            Input::PointerMove { position, .. } => {
                let previous = self.pointer.position.unwrap_or(position);
                let delta = LogicalPoint {
                    x: position.x - previous.x,
                    y: position.y - previous.y,
                };
                self.pointer.position = Some(position);
                self.pointer.event = PointerEvent::Move(delta);
                if self.pointer.down && !self.pointer.dragging {
                    let x = position.x - self.pointer.origin.x;
                    let y = position.y - self.pointer.origin.y;
                    if x * x + y * y >= DRAG_THRESHOLD * DRAG_THRESHOLD {
                        self.pointer.dragging = true;
                        let previous = self.active;
                        self.active = self.drag_owner;
                        if self.active != previous {
                            self.deactivated = previous;
                            self.activated = self.active;
                        }
                    }
                }
            }
            Input::PointerUp {
                position,
                button: PointerButton::Primary,
                leave,
                ..
            } => {
                self.pointer.position = Some(position);
                self.pointer.down = false;
                self.pointer.event = PointerEvent::Up { leave };
                self.deactivated = self.active;
            }
            Input::PointerUp { position, .. } => self.pointer.position = Some(position),
            Input::PointerLeave => self.pointer.position = None,
            Input::Scroll {
                position,
                delta_x,
                delta_y,
                continuous,
                phase,
                ..
            } => {
                self.pointer.position = Some(position);
                self.pointer.event = PointerEvent::Scroll {
                    delta: LogicalPoint {
                        x: delta_x,
                        y: delta_y,
                    },
                    continuous,
                    phase,
                };
            }
            _ => {}
        }

        let position = self.pointer.position;
        let hovered = position.and_then(|position| Self::hit(&self.previous_hits, position));
        self.hovered = hovered.map(|item| item.id);
        self.scroll_owner = position.and_then(|position| {
            self.previous_hits
                .iter()
                .rev()
                .find(|item| item.sense.scroll && item.area.contains(position.x, position.y))
                .map(|item| item.id)
        });

        if matches!(self.pointer.event, PointerEvent::Down) {
            let previous = self.active;
            self.active = hovered.filter(|item| item.sense.click).map(|item| item.id);
            if self.active != previous {
                self.deactivated = previous;
                self.activated = self.active;
            }
            self.focused = hovered.filter(|item| item.sense.focus).map(|item| item.id);
            self.drag_owner = position.and_then(|position| {
                self.previous_hits
                    .iter()
                    .rev()
                    .find(|item| item.sense.drag && item.area.contains(position.x, position.y))
                    .map(|item| item.id)
            });
        }
    }

    pub fn response(&mut self, id: WidgetId, sense: Sense) -> Interaction {
        #[cfg(debug_assertions)]
        assert!(self.seen.insert(id), "duplicate WidgetId {id:?}");
        self.requests.push((id, sense));

        let active = self.active == Some(id);
        let hovered = self.hovered == Some(id);
        Interaction {
            hovered,
            active: active && self.pointer.down,
            activated: self.activated == Some(id),
            deactivated: self.deactivated == Some(id),
            clicked: active
                && hovered
                && matches!(self.pointer.event, PointerEvent::Up { .. })
                && !self.pointer.dragging,
            dragging: active && self.pointer.down && self.pointer.dragging,
            drag_delta: match self.pointer.event {
                PointerEvent::Move(delta) if active && self.pointer.dragging => delta,
                _ => LogicalPoint::default(),
            },
            scroll: match self.pointer.event {
                PointerEvent::Scroll {
                    delta,
                    continuous,
                    phase,
                } if self.scroll_owner == Some(id) => Some(ScrollInteraction {
                    delta,
                    continuous,
                    phase,
                }),
                _ => None,
            },
        }
    }

    pub fn register_hits(
        &mut self,
        hits: impl IntoIterator<Item = (WidgetId, Option<LogicalRect>)>,
    ) {
        self.requests.sort_unstable_by_key(|request| request.0);
        for (id, area) in hits {
            if let Ok(index) = self.requests.binary_search_by_key(&id, |request| request.0) {
                self.current_hits.push(HitItem {
                    id,
                    area: area.unwrap_or_default(),
                    sense: self.requests[index].1,
                });
            }
        }
    }

    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.focused == Some(id)
    }

    pub fn focus(&mut self, id: WidgetId) -> bool {
        if self.focused == Some(id) {
            return false;
        }
        self.focused = Some(id);
        true
    }

    pub fn clear_focus(&mut self) -> bool {
        self.focused.take().is_some()
    }

    pub fn pointer_position(&self) -> Option<LogicalPoint> {
        self.pointer.position
    }

    pub fn end_frame(&mut self) -> bool {
        if self
            .active
            .is_some_and(|id| !self.current_hits.iter().any(|item| item.id == id))
        {
            self.active = None;
        }
        if self
            .focused
            .is_some_and(|id| !self.current_hits.iter().any(|item| item.id == id))
        {
            self.focused = None;
        }
        if let PointerEvent::Up { leave } = self.pointer.event {
            self.active = None;
            self.drag_owner = None;
            self.pointer.dragging = false;
            if leave {
                self.pointer.position = None;
            }
        }

        std::mem::swap(&mut self.previous_hits, &mut self.current_hits);
        self.current_hits.clear();

        let next_hovered = self
            .pointer
            .position
            .and_then(|position| Self::hit(&self.previous_hits, position));
        next_hovered.map(|item| item.id) != self.hovered
    }

    fn hit(hits: &[HitItem], position: LogicalPoint) -> Option<HitItem> {
        hits.iter()
            .rev()
            .find(|item| item.area.contains(position.x, position.y))
            .copied()
    }
}
