use std::{f32::consts::PI, time::Duration};

use super::{Frame, Request};
use crate::{
    Context, Direction, Drag, Interaction, InteractionConfig, Phase, Pinch, Rotation, Scalar, Scroll, TouchPhase,
    geometry::{Point, Rect, Sides},
    input::{Input, PointerButton, ScrollPhase},
    interact::{Sense, WidgetId},
    macros::flags,
};

pub fn resolve<C: Context>(frame: &mut Frame<C>) {
    frame.geometry_previous.clear();
    frame.interaction.hits.clear();
    if !frame.requests.is_empty() {
        for index in 0..frame.nodes.len() {
            let node = frame.paint_order.get(index).map_or(index, |id| id.index());
            let stored = &frame.nodes[node];
            let id = stored.widget_id;
            let Some(request) = frame.requests.get(&id).copied() else {
                continue;
            };
            let area = frame.node_geometry[node].area;
            frame.geometry_previous.push((id, area));
            let Request::Interaction(sense) = request else {
                continue;
            };
            let hit = stored
                .geometry
                .index()
                .map_or(Sides::all(C::Scalar::ZERO), |index| frame.geometry[index].hit);
            let area = Rect::new(
                area.x - hit.left,
                area.y - hit.top,
                area.width + hit.left + hit.right,
                area.height + hit.top + hit.bottom,
            );
            let clip_bounds = frame.clip_bounds(stored.resolved_clip);
            // todo: test interaction against the actual custom clip chain
            if let Some(area) = area.intersection(clip_bounds) {
                frame.interaction.hits.push(HitItem {
                    id,
                    area,
                    sense,
                    node,
                    end: stored.subtree_end as usize,
                });
            }
        }
    }
    if frame.interaction.end(&frame.input) {
        frame.frame_requested = true;
    }
}

#[derive(Default)]
pub struct InteractionState<T> {
    pub config: InteractionConfig,
    focused: Option<WidgetId>,
    hovered: Option<WidgetId>,
    position: Option<Point<T>>,
    mouse: Option<Press<T>>,
    scroll_owner: Option<WidgetId>,
    touches: Vec<Touch<T>>,
    transforms: Vec<Transform>,
    taps: Vec<Tap<T>>,
    responses: Vec<StoredInteraction<T>>,
    hits: Vec<HitItem<T>>,
}

impl<T: Scalar> InteractionState<T> {
    pub fn begin(&mut self, input: &Input<T>, time: Duration) -> bool {
        self.responses.clear();
        if let Some(owner) = self.mouse.as_ref().and_then(|press| press.active) {
            self.emit(owner).state |= Flags::PREVIOUS_ACTIVE;
        }
        for index in 0..self.touches.len() {
            if let Some(owner) = self.touches[index].press.active {
                self.emit(owner).state |= Flags::PREVIOUS_ACTIVE;
            }
        }
        self.taps
            .retain(|tap| time.saturating_sub(tap.time) <= self.config.double_tap_delay);

        match *input {
            Input::PointerDown {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                self.position = Some(position);
                if let Some(mut press) = self.mouse.take() {
                    self.cancel_press(&mut press);
                }
                self.mouse = Some(self.press(position, time, false));
            }
            Input::PointerMove { position, .. }
            | Input::PointerUp {
                position,
                button: PointerButton::Primary,
                ..
            } => {
                self.position = Some(position);
                let released = matches!(input, Input::PointerUp { .. });
                if let Some(mut press) = self.mouse.take() {
                    self.update_press(&mut press, position, time, released);
                    if !released {
                        self.mouse = Some(press);
                    }
                }
            }
            Input::PointerDown { position, .. } | Input::PointerUp { position, .. } => {
                self.position = Some(position);
            }
            Input::PointerLeave => self.position = None,
            Input::Scroll {
                position,
                delta_x,
                delta_y,
                continuous,
                phase,
                ..
            } => {
                self.position = Some(position);
                let anchor = self.hit(position).map(|hit| hit.id);
                let axis = if delta_x == 0.0 && delta_y == 0.0 {
                    Sense::SCROLL
                } else if delta_x.abs() > delta_y.abs() {
                    Sense::SCROLL_X
                } else {
                    Sense::SCROLL_Y
                };
                let owner = if continuous && phase != ScrollPhase::Started {
                    self.scroll_owner
                } else {
                    None
                }
                .or_else(|| self.target(anchor, position, axis).map(|hit| hit.id));
                self.scroll_owner =
                    owner.filter(|_| continuous && matches!(phase, ScrollPhase::Started | ScrollPhase::Moved));
                if let Some(owner) = owner {
                    let response = self.emit(owner);
                    response.set_scroll(
                        match phase {
                            ScrollPhase::Started => Phase::Started,
                            ScrollPhase::Moved => Phase::Changed,
                            ScrollPhase::Ended => Phase::Ended,
                            ScrollPhase::Cancelled => Phase::Cancelled,
                        },
                        Point::new(delta_x, delta_y),
                        continuous,
                    );
                }
            }
            Input::Touch { id, position, phase } => match (phase, self.touches.iter().position(|touch| touch.id == id))
            {
                (TouchPhase::Down, index) => {
                    if let Some(index) = index {
                        self.cancel_touch(index);
                    }
                    let press = self.press(position, time, true);
                    self.touches.push(Touch { id, press });
                    let hit = self
                        .hits
                        .iter()
                        .rev()
                        .find(|hit| {
                            hit.sense.intersects(Sense::PINCH | Sense::ROTATE)
                                && self.is_target(hit, press.anchor, position)
                                && self.touches.iter().any(|touch| {
                                    touch.id != id
                                        && (!touch.press.consumed
                                            || touch.press.capture == Some(Capture::Multi(hit.id)))
                                        && self.is_target(hit, touch.press.anchor, touch.press.origin)
                                })
                        })
                        .copied();
                    if let Some(hit) = hit {
                        let owner = hit.id;
                        for index in 0..self.touches.len() {
                            let mut touch = self.touches[index];
                            if (touch.press.consumed && touch.press.capture != Some(Capture::Multi(owner)))
                                || !self.is_target(&hit, touch.press.anchor, touch.press.origin)
                            {
                                continue;
                            }
                            if matches!(touch.press.capture, Some(Capture::Single(..))) {
                                self.touches[index].press.capture = None;
                                self.cancel_press(&mut touch.press);
                            }
                            touch.press.capture = Some(Capture::Multi(owner));
                            touch.press.consumed = true;
                            touch.press.active = Some(owner);
                            self.touches[index] = touch;
                        }
                        self.update_transform(owner, time, true);
                    }
                }
                (TouchPhase::Cancel, Some(index)) => self.cancel_touch(index),
                (TouchPhase::Move | TouchPhase::Up, Some(index)) => {
                    let released = phase == TouchPhase::Up;
                    if let Some(Capture::Multi(owner)) = self.touches[index].press.capture {
                        self.touches[index].press.position = position;
                        self.update_transform(owner, time, false);
                        if released {
                            self.touches.remove(index);
                            self.update_transform(owner, time, true);
                        }
                    } else {
                        let mut touch = self.touches[index];
                        if released {
                            self.touches.remove(index);
                        }
                        self.update_press(&mut touch.press, position, time, released);
                        if !released {
                            self.touches[index] = touch;
                        }
                    }
                }
                _ => {}
            },
            Input::Cancel => {
                if let Some(owner) = self.scroll_owner.take() {
                    self.emit(owner).set_scroll(Phase::Cancelled, Point::ZERO, true);
                }
                if let Some(mut press) = self.mouse.take() {
                    self.cancel_press(&mut press);
                }
                while !self.touches.is_empty() {
                    self.cancel_touch(self.touches.len() - 1);
                }
                self.position = None;
                self.taps.clear();
            }
            Input::None | Input::Text(_) | Input::Key(_) => {}
        }

        self.hovered = self.position.and_then(|position| self.hit(position)).map(|hit| hit.id);
        if let Some(mut press) = self.mouse.take() {
            self.hold(&mut press, time);
            if let Some(owner) = press.active {
                self.emit(owner).state |= Flags::ACTIVE;
            }
            self.mouse = Some(press);
        }
        for index in 0..self.touches.len() {
            let mut touch = self.touches[index];
            self.hold(&mut touch.press, time);
            if let Some(owner) = touch.press.active {
                self.emit(owner).state |= Flags::ACTIVE;
            }
            self.touches[index] = touch;
        }
        for response in &mut self.responses {
            let previous = response.state.contains(Flags::PREVIOUS_ACTIVE);
            let active = response.state.contains(Flags::ACTIVE);
            if active != previous {
                response.state |= if active { Flags::ACTIVATED } else { Flags::DEACTIVATED };
            }
        }
        self.responses.iter().any(|response| {
            response.state.intersects(Flags::EVENTS)
                || response.motion_phase.is_some()
                || response.scroll_phase.is_some()
        })
    }

    pub fn response(&self, id: WidgetId) -> Interaction<T> {
        let mut response = self
            .responses
            .iter()
            .find(|response| response.id == id)
            .copied()
            .unwrap_or_else(|| StoredInteraction::new(id));
        if (self.mouse.is_some() || !self.touches.is_empty()) && response.motion_phase.is_none() {
            if let Some(transform) = self.transforms.iter().find(|transform| transform.owner == id) {
                response.set_motion(
                    Phase::Changed,
                    Motion {
                        center: Point::new(T::from_f32(transform.center.x), T::from_f32(transform.center.y)),
                        ..Motion::default()
                    },
                    transform.sense,
                );
            } else if let Some((press, sense)) = self.presses().find_map(|press| match press.capture {
                Some(Capture::Single(owner, sense)) if owner == id => Some((press, sense)),
                _ => None,
            }) {
                response.set_pan(press, sense, Phase::Changed, Point::ZERO);
                response.motion.velocity = Point::ZERO;
            }
        }
        response.snapshot(self.hovered == Some(id))
    }

    pub fn next_deadline(&self) -> Option<Duration> {
        self.presses()
            .filter(|press| press.hold.is_some() && !press.moved && !press.held && !press.consumed)
            .filter_map(|press| press.start.checked_add(self.config.long_press_delay))
            .min()
    }

    pub fn focused(&self) -> Option<WidgetId> {
        self.focused
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

    pub fn pointer_position(&self) -> Option<Point<T>> {
        self.position
    }

    pub fn end(&mut self, input: &Input<T>) -> bool {
        let exists = |id| self.hits.iter().any(|hit| hit.id == id);
        let valid = |press: &Press<T>| press.active.or(press.anchor).is_none_or(exists);
        if self.focused.is_some_and(|id| !exists(id)) {
            self.focused = None;
        }
        self.mouse = self.mouse.filter(valid);
        self.scroll_owner = self.scroll_owner.filter(|id| exists(*id));
        self.transforms.retain(|transform| exists(transform.owner));
        for touch in &mut self.touches {
            if !valid(&touch.press) {
                touch.press.active = None;
                touch.press.capture = None;
                touch.press.hold = None;
                touch.press.consumed = true;
            }
        }
        self.taps.retain(|tap| exists(tap.id));
        if matches!(
            input,
            Input::PointerUp {
                button: PointerButton::Primary,
                leave: true,
                ..
            }
        ) {
            self.position = None;
        }
        self.position.and_then(|position| self.hit(position)).map(|hit| hit.id) != self.hovered
    }

    fn press(&mut self, position: Point<T>, time: Duration, touch: bool) -> Press<T> {
        let hit = self.hit(position);
        let anchor = hit.map(|hit| hit.id);
        let sense = hit.map_or(Sense::NONE, |hit| hit.sense);
        if touch && self.touches.iter().any(|touch| touch.press.anchor == anchor) {
            self.taps.retain(|tap| !tap.touch || Some(tap.id) != anchor);
        }
        self.focused = hit.filter(|hit| hit.sense.contains(Sense::FOCUS)).map(|hit| hit.id);
        let mut press = Press {
            anchor,
            active: hit
                .filter(|hit| {
                    hit.sense
                        .intersects(Sense::CLICK | Sense::DOUBLE_TAP | Sense::LONG_PRESS)
                })
                .map(|hit| hit.id),
            sense,
            hold: self.target(anchor, position, Sense::LONG_PRESS).map(|hit| hit.id),
            capture: None,
            origin: position,
            position,
            start: time,
            last: time,
            velocity: Point::ZERO,
            moved: false,
            held: false,
            consumed: false,
            touch,
        };
        if !touch
            && press.active.is_none()
            && let Some(hit) = self.target(anchor, position, Sense::DRAG).filter(|hit| {
                !hit.sense
                    .intersects(Sense::CLICK | Sense::DOUBLE_TAP | Sense::LONG_PRESS)
            })
        {
            press.capture = Some(Capture::Single(hit.id, hit.sense));
            press.moved = true;
            press.active = Some(hit.id);
            self.emit(hit.id)
                .set_pan(&press, hit.sense, Phase::Started, Point::ZERO);
        }
        press
    }

    fn update_press(&mut self, press: &mut Press<T>, position: Point<T>, time: Duration, released: bool) {
        let delta = Point::new(position.x - press.position.x, position.y - press.position.y);
        let elapsed = time.saturating_sub(press.last);
        let seconds = elapsed.as_secs_f32();
        if seconds > 0.0 && (!released || delta != Point::ZERO) {
            press.velocity = Point::new(delta.x.to_f32() / seconds, delta.y.to_f32() / seconds);
        } else if released && elapsed > Duration::from_millis(100) {
            press.velocity = Point::ZERO;
        }
        press.position = position;
        if !released {
            press.last = time;
        }
        if press.consumed && press.capture.is_none() {
            return;
        }
        let x = (position.x - press.origin.x).to_f32();
        let y = (position.y - press.origin.y).to_f32();
        let horizontal = x.abs() >= y.abs();
        press.moved |= x * x + y * y >= self.config.drag_threshold * self.config.drag_threshold;
        let mut phase = if released { Phase::Ended } else { Phase::Changed };
        let starting = press.moved && press.capture.is_none();
        if starting {
            let axis = if horizontal {
                Sense::DRAG_X | Sense::SWIPE_X | if press.touch { Sense::SCROLL_X } else { Sense::NONE }
            } else {
                Sense::DRAG_Y | Sense::SWIPE_Y | if press.touch { Sense::SCROLL_Y } else { Sense::NONE }
            };
            let target = self
                .target(press.anchor, press.origin, axis)
                .filter(|hit| !press.held || press.hold == Some(hit.id));
            press.active = target.map(|hit| hit.id);
            if let Some(hit) = target {
                press.capture = Some(Capture::Single(hit.id, hit.sense));
                if !released {
                    phase = Phase::Started;
                }
            }
        }
        if released {
            self.hold(press, time);
        }
        if let Some(Capture::Single(owner, sense)) = press.capture {
            let delta = if starting {
                Point::new(position.x - press.origin.x, position.y - press.origin.y)
            } else {
                delta
            };
            if released
                && self
                    .presses()
                    .any(|press| matches!(press.capture, Some(Capture::Single(id, _)) if id == owner))
            {
                phase = Phase::Changed;
            }
            self.emit(owner).set_pan(press, sense, phase, delta);
            if !released || phase != Phase::Ended || press.held || press.consumed {
                return;
            }
            let axis = if horizontal { Sense::SWIPE_X } else { Sense::SWIPE_Y };
            let distance = if horizontal { x.abs() } else { y.abs() };
            let duration = time.saturating_sub(press.start).as_secs_f32();
            if sense.intersects(axis)
                && distance >= self.config.swipe_distance
                && distance >= self.config.swipe_speed * duration
            {
                let direction = if horizontal {
                    if x < 0.0 { Direction::Left } else { Direction::Right }
                } else if y < 0.0 {
                    Direction::Up
                } else {
                    Direction::Down
                };
                self.emit(owner).swipe = Some(direction);
            }
            return;
        }
        if !released || press.consumed || press.moved || press.held {
            return;
        }
        let Some(id) = press
            .anchor
            .filter(|id| self.hit(position).is_some_and(|hit| hit.id == *id))
        else {
            return;
        };
        if press.sense.contains(Sense::CLICK) {
            self.emit(id).state |= Flags::CLICKED;
        }
        if !press.sense.contains(Sense::DOUBLE_TAP)
            || (press.touch && self.touches.iter().any(|touch| touch.press.anchor == Some(id)))
        {
            return;
        }
        let previous = self
            .taps
            .iter()
            .position(|tap| tap.id == id && tap.touch == press.touch);
        let doubled = previous.is_some_and(|index| {
            let tap = self.taps.remove(index);
            let x = (position.x - tap.position.x).to_f32();
            let y = (position.y - tap.position.y).to_f32();
            tap.time <= press.start
                && x * x + y * y <= self.config.double_tap_distance * self.config.double_tap_distance
        });
        if doubled {
            self.emit(id).state |= Flags::DOUBLE_TAPPED;
        } else {
            self.taps.push(Tap {
                id,
                time,
                position,
                touch: press.touch,
            });
        }
    }

    fn hold(&mut self, press: &mut Press<T>, time: Duration) {
        if let Some(owner) = press.hold
            && !press.consumed
            && !press.moved
            && !press.held
            && time.saturating_sub(press.start) >= self.config.long_press_delay
        {
            press.held = true;
            press.active = Some(owner);
            self.emit(owner).state |= Flags::LONG_PRESSED;
        }
    }

    fn update_transform(&mut self, owner: WidgetId, time: Duration, rebase: bool) {
        let mut points = self
            .touches
            .iter()
            .filter(|touch| touch.press.capture == Some(Capture::Multi(owner)))
            .map(|touch| touch.press.position);
        let first = points.next();
        let second = points.next();
        let previous = self.transforms.iter().position(|transform| transform.owner == owner);
        let (Some(first), Some(second)) = (first, second) else {
            if let Some(index) = previous {
                let transform = self.transforms.remove(index);
                let continuing =
                    first.is_some() && transform.sense.intersects(Sense::DRAG | Sense::SCROLL | Sense::SWIPE);
                let response = self.emit(owner);
                response.set_motion(Phase::Ended, response.motion, transform.sense);
                if continuing && transform.sense.intersects(Sense::SCROLL) {
                    response.set_scroll(Phase::Started, Point::ZERO, true);
                }
                for touch in &mut self.touches {
                    if touch.press.capture == Some(Capture::Multi(owner)) {
                        if continuing {
                            touch.press.capture = Some(Capture::Single(owner, transform.sense));
                            touch.press.last = time;
                            touch.press.velocity = transform.velocity;
                        } else {
                            touch.press.capture = None;
                            touch.press.active = None;
                        }
                    }
                }
            }
            return;
        };
        let mut count = 2;
        let mut center = Point::new(
            first.x.to_f32() + second.x.to_f32(),
            first.y.to_f32() + second.y.to_f32(),
        );
        for point in points {
            center.x += point.x.to_f32();
            center.y += point.y.to_f32();
            count += 1;
        }
        center.x /= count as f32;
        center.y /= count as f32;
        let x = (second.x - first.x).to_f32();
        let y = (second.y - first.y).to_f32();
        let span = x.hypot(y);
        let angle = y.atan2(x);
        let sense = previous.map_or_else(
            || {
                self.hits
                    .iter()
                    .find(|hit| hit.id == owner)
                    .map_or(Sense::NONE, |hit| hit.sense)
            },
            |index| self.transforms[index].sense,
        );
        let mut motion = Motion {
            center: Point::new(T::from_f32(center.x), T::from_f32(center.y)),
            ..Motion::default()
        };
        if let Some(index) = previous
            && !rebase
        {
            let old = self.transforms[index];
            motion.delta = Point::new(
                T::from_f32(center.x - old.center.x),
                T::from_f32(center.y - old.center.y),
            );
            let elapsed = time.saturating_sub(old.time).as_secs_f32();
            motion.velocity = if elapsed > 0.0 {
                Point::new((center.x - old.center.x) / elapsed, (center.y - old.center.y) / elapsed)
            } else {
                old.velocity
            };
            if sense.contains(Sense::PINCH) && old.span > f32::EPSILON {
                motion.scale = span / old.span;
            }
            if sense.contains(Sense::ROTATE) && old.span > f32::EPSILON && span > f32::EPSILON {
                motion.rotation = (angle - old.angle + PI).rem_euclid(2.0 * PI) - PI;
            }
        }
        let transform = Transform {
            owner,
            sense,
            center,
            span,
            angle,
            time,
            velocity: motion.velocity,
        };
        if let Some(index) = previous {
            self.transforms[index] = transform;
        } else {
            self.transforms.push(transform);
        }
        if previous.is_none() || !rebase {
            self.emit(owner).set_motion(
                if previous.is_none() {
                    Phase::Started
                } else {
                    Phase::Changed
                },
                motion,
                sense,
            );
        }
    }

    fn cancel_touch(&mut self, index: usize) {
        let mut touch = self.touches.remove(index);
        let capture = touch.press.capture;
        self.cancel_press(&mut touch.press);
        if let Some(Capture::Multi(owner)) = capture
            && let Some(index) = self.transforms.iter().position(|transform| transform.owner == owner)
        {
            let transform = self.transforms.remove(index);
            self.emit(owner).set_motion(
                Phase::Cancelled,
                Motion {
                    center: Point::new(T::from_f32(transform.center.x), T::from_f32(transform.center.y)),
                    ..Motion::default()
                },
                transform.sense,
            );
            for touch in &mut self.touches {
                if touch.press.capture == Some(Capture::Multi(owner)) {
                    touch.press.active = None;
                    touch.press.consumed = true;
                    touch.press.capture = None;
                }
            }
        }
    }

    fn cancel_press(&mut self, press: &mut Press<T>) {
        if let Some(Capture::Single(owner, sense)) = press.capture {
            let continuing = self
                .presses()
                .any(|press| matches!(press.capture, Some(Capture::Single(id, _)) if id == owner));
            self.emit(owner).set_pan(
                press,
                sense,
                if continuing { Phase::Changed } else { Phase::Cancelled },
                Point::ZERO,
            );
        }
        press.active = None;
        press.capture = None;
        press.consumed = true;
    }

    fn emit(&mut self, id: WidgetId) -> &mut StoredInteraction<T> {
        let index = self
            .responses
            .iter()
            .position(|response| response.id == id)
            .unwrap_or_else(|| {
                self.responses.push(StoredInteraction::new(id));
                self.responses.len() - 1
            });
        &mut self.responses[index]
    }

    fn presses(&self) -> impl Iterator<Item = &Press<T>> {
        self.mouse.iter().chain(self.touches.iter().map(|touch| &touch.press))
    }

    fn hit(&self, position: Point<T>) -> Option<HitItem<T>> {
        self.hits.iter().rev().find(|hit| hit.area.contains(position)).copied()
    }

    fn target(&self, anchor: Option<WidgetId>, position: Point<T>, sense: Sense) -> Option<HitItem<T>> {
        let node = anchor.and_then(|id| self.hits.iter().find(|hit| hit.id == id))?.node;
        self.hits
            .iter()
            .rev()
            .find(|hit| {
                hit.sense.intersects(sense) && hit.area.contains(position) && hit.node <= node && node <= hit.end
            })
            .copied()
    }

    fn is_target(&self, hit: &HitItem<T>, anchor: Option<WidgetId>, position: Point<T>) -> bool {
        hit.area.contains(position)
            && anchor
                .and_then(|id| self.hits.iter().find(|hit| hit.id == id))
                .is_some_and(|anchor| hit.node <= anchor.node && anchor.node <= hit.end)
    }
}

#[derive(Clone, Copy)]
struct HitItem<T> {
    id: WidgetId,
    area: Rect<T>,
    sense: Sense,
    node: usize,
    end: usize,
}

#[derive(Clone, Copy)]
struct Press<T> {
    anchor: Option<WidgetId>,
    active: Option<WidgetId>,
    sense: Sense,
    hold: Option<WidgetId>,
    capture: Option<Capture>,
    origin: Point<T>,
    position: Point<T>,
    start: Duration,
    last: Duration,
    velocity: Point<f32>,
    moved: bool,
    held: bool,
    consumed: bool,
    touch: bool,
}

#[derive(Clone, Copy)]
struct Touch<T> {
    id: u64,
    press: Press<T>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    // drag, scroll or swipe
    Single(WidgetId, Sense),
    // grouped touches for pinch or rotation
    Multi(WidgetId),
}

#[derive(Clone, Copy)]
struct Transform {
    owner: WidgetId,
    sense: Sense,
    center: Point<f32>,
    span: f32,
    angle: f32,
    time: Duration,
    velocity: Point<f32>,
}

#[derive(Clone, Copy)]
struct Tap<T> {
    id: WidgetId,
    time: Duration,
    position: Point<T>,
    touch: bool,
}

#[derive(Clone, Copy)]
struct StoredInteraction<T> {
    id: WidgetId,
    motion: Motion<T>,
    scroll_delta: Point<f32>,
    motion_sense: Sense,
    state: Flags,
    motion_phase: Option<Phase>,
    drag_phase: Option<Phase>,
    scroll_phase: Option<Phase>,
    swipe: Option<Direction>,
}

impl<T: Scalar> StoredInteraction<T> {
    fn new(id: WidgetId) -> Self {
        Self {
            id,
            motion: Motion::default(),
            scroll_delta: Point::ZERO,
            motion_sense: Sense::NONE,
            state: Flags::default(),
            motion_phase: None,
            drag_phase: None,
            scroll_phase: None,
            swipe: None,
        }
    }

    fn snapshot(self, hovered: bool) -> Interaction<T> {
        Interaction {
            active: self.state.contains(Flags::ACTIVE),
            activated: self.state.contains(Flags::ACTIVATED),
            deactivated: self.state.contains(Flags::DEACTIVATED),
            hovered,
            clicked: self.state.contains(Flags::CLICKED),
            double_tapped: self.state.contains(Flags::DOUBLE_TAPPED),
            long_pressed: self.state.contains(Flags::LONG_PRESSED),
            drag: self.drag_phase.map(|phase| Drag {
                phase,
                position: self.motion.center,
                delta: self.motion.delta,
                velocity: self.motion.velocity,
            }),
            scroll: self.scroll_phase.map(|phase| Scroll {
                phase,
                delta: self.scroll_delta,
                continuous: self.state.contains(Flags::CONTINUOUS_SCROLL),
            }),
            swipe: self.swipe,
            pinch: self.motion_phase.and_then(|phase| {
                self.motion_sense.contains(Sense::PINCH).then_some(Pinch {
                    phase,
                    center: self.motion.center,
                    scale: self.motion.scale,
                })
            }),
            rotation: self.motion_phase.and_then(|phase| {
                self.motion_sense.contains(Sense::ROTATE).then_some(Rotation {
                    phase,
                    center: self.motion.center,
                    radians: self.motion.rotation,
                })
            }),
        }
    }

    fn set_scroll(&mut self, phase: Phase, delta: Point<f32>, continuous: bool) {
        self.scroll_phase = Some(phase);
        self.state &= !Flags::CONTINUOUS_SCROLL;
        if continuous {
            self.state |= Flags::CONTINUOUS_SCROLL;
        }
        self.scroll_delta = delta;
    }

    fn set_motion(&mut self, phase: Phase, motion: Motion<T>, sense: Sense) {
        self.motion_phase = Some(phase);
        self.swipe = None;
        self.motion = motion;
        self.motion_sense = sense;
    }

    fn set_pan(&mut self, press: &Press<T>, sense: Sense, phase: Phase, delta: Point<T>) {
        self.drag_phase = sense.intersects(Sense::DRAG).then_some(phase);
        self.set_motion(
            phase,
            Motion {
                center: press.position,
                delta,
                velocity: press.velocity,
                ..Motion::default()
            },
            Sense::NONE,
        );
        if press.touch && sense.intersects(Sense::SCROLL) && (phase != Phase::Changed || self.scroll_phase.is_none()) {
            self.set_scroll(
                phase,
                Point::new(
                    if sense.intersects(Sense::SCROLL_X) {
                        -delta.x.to_f32()
                    } else {
                        0.0
                    },
                    if sense.intersects(Sense::SCROLL_Y) {
                        -delta.y.to_f32()
                    } else {
                        0.0
                    },
                ),
                true,
            );
        }
    }
}

flags! {
    struct Flags(pub u8) {
        ACTIVE = 1,
        ACTIVATED = 1 << 1,
        DEACTIVATED = 1 << 2,
        CLICKED = 1 << 3,
        DOUBLE_TAPPED = 1 << 4,
        LONG_PRESSED = 1 << 5,
        CONTINUOUS_SCROLL = 1 << 6,
        PREVIOUS_ACTIVE = 1 << 7,
        EVENTS = Self::ACTIVATED.0 | Self::DEACTIVATED.0 | Self::CLICKED.0
            | Self::DOUBLE_TAPPED.0 | Self::LONG_PRESSED.0,
    }
}

#[derive(Clone, Copy)]
struct Motion<T> {
    center: Point<T>,
    delta: Point<T>,
    velocity: Point<f32>,
    scale: f32,
    rotation: f32,
}

impl<T: Scalar> Default for Motion<T> {
    fn default() -> Self {
        Self {
            center: Point::ZERO,
            delta: Point::ZERO,
            velocity: Point::ZERO,
            scale: 1.0,
            rotation: 0.0,
        }
    }
}
