use crate::{
    geometry::{Point, Rect},
    input::{Input, PointerButton},
    interact::{Interaction, ScrollInteraction, Sense, WidgetId},
    renderer::Renderer,
};

use super::Frame;

const DRAG_THRESHOLD: f32 = 6.0;

pub fn resolve<R: Renderer>(frame: &mut Frame<R>, renderer: &R) {
    frame.interaction.sort_requests();
    if !frame.paint_order.is_empty() {
        frame
            .order_stack
            .resize(frame.nodes.len(), frame.node_id(0));
        for (rank, node) in frame.paint_order.iter().copied().enumerate() {
            frame.order_stack[node.index()] = frame.node_id(rank);
        }
        let ranks = &frame.order_stack;
        frame
            .geometry
            .sort_unstable_by_key(|record| ranks[record.node.index()].index());
    }
    for index in 0..frame.geometry.len() {
        let record = frame.geometry[index];
        let Some(id) = record.id else {
            continue;
        };
        let stored = &frame.nodes[record.node.index()];
        frame.geometry_current.push((id, stored.area));
        let area = Rect::new(
            stored.area.x - record.hit.left,
            stored.area.y - record.hit.top,
            stored.area.width + record.hit.left + record.hit.right,
            stored.area.height + record.hit.top + record.hit.bottom,
        );
        // todo: test interaction against the actual custom clip chain
        frame
            .interaction
            .register(id, renderer.interaction_area(area, stored.clip_bounds));
    }
    if frame.interaction.end() {
        frame.frame_requested = true;
    }
}

#[derive(Default)]
pub struct InteractionState {
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
    origin: Point,
    position: Option<Point>,
    down: bool,
    dragging: bool,
    event: PointerEvent,
}

#[derive(Clone, Copy, Default)]
enum PointerEvent {
    #[default]
    None,
    Down,
    Move(Point),
    Up {
        leave: bool,
    },
    Scroll(ScrollInteraction),
}

#[derive(Clone, Copy)]
struct HitItem {
    id: WidgetId,
    area: Rect,
    sense: Sense,
}

impl InteractionState {
    pub fn begin(&mut self, input: &Input) {
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
                let delta = Point::new(position.x - previous.x, position.y - previous.y);
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
                self.pointer.event = PointerEvent::Scroll(ScrollInteraction {
                    delta: Point::new(delta_x, delta_y),
                    continuous,
                    phase,
                });
            }
            Input::None | Input::Text(_) | Input::Key(_) => {}
        }

        let position = self.pointer.position;
        let hovered = position.and_then(|position| Self::hit(&self.previous_hits, position));
        self.hovered = hovered.map(|item| item.id);
        self.scroll_owner = position.and_then(|position| {
            self.previous_hits
                .iter()
                .rev()
                .find(|item| item.sense.scroll && item.area.contains(position))
                .map(|item| item.id)
        });

        if matches!(self.pointer.event, PointerEvent::Down) {
            let previous = self.active;
            self.active = hovered.filter(|item| item.sense.click).map(|item| item.id);
            self.focused = hovered.filter(|item| item.sense.focus).map(|item| item.id);
            let drag_owner = position.and_then(|position| {
                self.previous_hits
                    .iter()
                    .rev()
                    .find(|item| item.sense.drag && item.area.contains(position))
                    .copied()
            });
            self.drag_owner = drag_owner.map(|item| item.id);
            if drag_owner.is_some_and(|item| item.sense == Sense::DRAG) {
                self.pointer.dragging = true;
                self.active = self.drag_owner;
            }
            if self.active != previous {
                self.deactivated = previous;
                self.activated = self.active;
            }
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
                _ => Point::ZERO,
            },
            scroll: match self.pointer.event {
                PointerEvent::Scroll(scroll) if self.scroll_owner == Some(id) => Some(scroll),
                _ => None,
            },
        }
    }

    pub fn register(&mut self, id: WidgetId, area: Option<Rect>) {
        if let Ok(index) = self.requests.binary_search_by_key(&id, |request| request.0) {
            self.current_hits.push(HitItem {
                id,
                area: area.unwrap_or_default(),
                sense: self.requests[index].1,
            });
        }
    }

    pub fn sort_requests(&mut self) {
        self.requests.sort_unstable_by_key(|request| request.0);
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

    pub fn pointer_position(&self) -> Option<Point> {
        self.pointer.position
    }

    pub fn end(&mut self) -> bool {
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

    fn hit(hits: &[HitItem], position: Point) -> Option<HitItem> {
        hits.iter()
            .rev()
            .find(|item| item.area.contains(position))
            .copied()
    }
}
