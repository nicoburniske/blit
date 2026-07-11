use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{Input, LogicalPoint, PhysicalPoint, PhysicalRect};

static NEXT_ID: AtomicU32 = AtomicU32::new(1);
const DRAG_THRESHOLD: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    click: bool,
    drag: bool,
    focus: bool,
}

impl Sense {
    pub const CLICK: Self = Self {
        click: true,
        drag: false,
        focus: false,
    };

    pub const DRAG: Self = Self {
        click: false,
        drag: true,
        focus: false,
    };

    pub const FOCUS: Self = Self {
        click: true,
        drag: false,
        focus: true,
    };

    pub const CLICK_AND_DRAG: Self = Self {
        click: true,
        drag: true,
        focus: false,
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Interaction {
    pub hovered: bool,
    pub pressed: bool,
    pub clicked: bool,
    pub dragged: bool,
    pub drag_delta: LogicalPoint,
    pub scroll_delta: LogicalPoint,
}

#[derive(Default)]
pub struct InteractionState {
    active: Option<WidgetId>,
    focused: Option<WidgetId>,
    hovered: Option<WidgetId>,
    drag_owner: Option<WidgetId>,
    scroll_owner: Option<WidgetId>,
    pointer: PointerState,
    previous_hits: Vec<HitItem>,
    current_hits: Vec<HitItem>,
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
    Scroll(LogicalPoint),
}

#[derive(Clone, Copy)]
struct HitItem {
    id: WidgetId,
    area: PhysicalRect,
    sense: Sense,
}

impl InteractionState {
    pub fn begin_frame(&mut self, input: &Input, scale_factor: f32) -> [Option<PhysicalRect>; 2] {
        let previous_hovered = self.hovered;
        let previous_area = previous_hovered.and_then(|id| {
            self.previous_hits
                .iter()
                .find(|item| item.id == id)
                .map(|item| item.area)
        });
        #[cfg(debug_assertions)]
        self.seen.clear();

        self.pointer.event = PointerEvent::None;

        match *input {
            Input::PointerDown { position } => {
                self.pointer.origin = position;
                self.pointer.position = Some(position);
                self.pointer.down = true;
                self.pointer.dragging = false;
                self.pointer.event = PointerEvent::Down;
            }
            Input::PointerMove { position } => {
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
                        self.active = self.drag_owner;
                    }
                }
            }
            Input::PointerUp { position, leave } => {
                self.pointer.position = Some(position);
                self.pointer.down = false;
                self.pointer.event = PointerEvent::Up { leave };
            }
            Input::PointerLeave => self.pointer.position = None,
            Input::Scroll {
                position,
                delta_x,
                delta_y,
            } => {
                self.pointer.position = Some(position);
                self.pointer.event = PointerEvent::Scroll(LogicalPoint {
                    x: delta_x,
                    y: delta_y,
                });
            }
            _ => {}
        }

        let position = self.physical_position(scale_factor);
        let hovered = position.and_then(|position| Self::hit(&self.previous_hits, position));
        self.hovered = hovered.map(|item| item.id);
        self.scroll_owner = position.and_then(|position| {
            self.previous_hits
                .iter()
                .rev()
                .find(|item| item.sense.drag && item.area.contains(position.x, position.y))
                .map(|item| item.id)
        });

        if matches!(self.pointer.event, PointerEvent::Down) {
            self.active = hovered.filter(|item| item.sense.click).map(|item| item.id);
            self.focused = hovered.filter(|item| item.sense.focus).map(|item| item.id);
            self.drag_owner = self.scroll_owner;
        }
        let current_area = hovered.map(|item| item.area);
        if previous_hovered == self.hovered {
            [None, None]
        } else {
            [previous_area, current_area]
        }
    }

    pub fn interact(
        &mut self,
        id: WidgetId,
        area: Option<PhysicalRect>,
        sense: Sense,
    ) -> Interaction {
        #[cfg(debug_assertions)]
        assert!(self.seen.insert(id), "duplicate WidgetId {id:?}");

        self.current_hits.push(HitItem {
            id,
            area: area.unwrap_or_default(),
            sense,
        });

        let active = self.active == Some(id);
        let hovered = self.hovered == Some(id);
        Interaction {
            hovered,
            pressed: active && self.pointer.down && !self.pointer.dragging,
            clicked: active
                && hovered
                && matches!(self.pointer.event, PointerEvent::Up { .. })
                && !self.pointer.dragging,
            dragged: active && self.pointer.dragging,
            drag_delta: match self.pointer.event {
                PointerEvent::Move(delta) if active && self.pointer.dragging => delta,
                _ => LogicalPoint::default(),
            },
            scroll_delta: match self.pointer.event {
                PointerEvent::Scroll(delta) if self.scroll_owner == Some(id) => delta,
                _ => LogicalPoint::default(),
            },
        }
    }

    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.focused == Some(id)
    }

    pub fn pointer_position(&self) -> Option<LogicalPoint> {
        self.pointer.position
    }

    pub fn end_frame(&mut self, scale_factor: f32) -> [Option<PhysicalRect>; 2] {
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

        let hovered_area = self.hovered.and_then(|id| {
            self.previous_hits
                .iter()
                .find(|item| item.id == id)
                .map(|item| item.area)
        });
        std::mem::swap(&mut self.previous_hits, &mut self.current_hits);
        self.current_hits.clear();

        let next_hovered = self
            .physical_position(scale_factor)
            .and_then(|position| Self::hit(&self.previous_hits, position));
        if next_hovered.map(|item| item.id) == self.hovered {
            return [None, None];
        }

        [hovered_area, next_hovered.map(|item| item.area)]
    }

    fn physical_position(&self, scale_factor: f32) -> Option<PhysicalPoint> {
        self.pointer.position.map(|position| PhysicalPoint {
            x: (position.x * scale_factor).floor() as i32,
            y: (position.y * scale_factor).floor() as i32,
        })
    }

    fn hit(hits: &[HitItem], position: PhysicalPoint) -> Option<HitItem> {
        hits.iter()
            .rev()
            .find(|item| item.area.contains(position.x, position.y))
            .copied()
    }
}
