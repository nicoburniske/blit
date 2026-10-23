use std::time::Duration;

use super::{Frame, Request, ResolvedClip, ResolvedClipId};
use crate::{
    Contact, ContactId, ContactSource, Context, Drag, Id, Input, Interaction, InteractionConfig, Modifiers, Phase,
    PointerButton, Scalar, Scroll, ScrollPhase, Sense, TouchPhase,
    geometry::{Point, Rect, Sides},
};

#[derive(Default)]
pub struct InteractionState<T> {
    pub config: InteractionConfig,
    pub clips: Vec<ResolvedClip<T>>,
    contacts: Vec<TrackedContact<T>>,
    next_contact: u64,
    cancellations: Vec<Cancellation<T>>,
    wheel: Option<Wheel>,
    hits: Vec<HitItem<T>>,
    focused: Option<Id>,
    hovered: Vec<Id>,
    position: Option<Point<T>>,
}

impl<T: Scalar> InteractionState<T> {
    pub fn contacts(&mut self, id: Id) -> impl Iterator<Item = Contact<T>> + '_ {
        let hits = &self.hits;
        let contacts = &self.contacts;
        self.cancellations
            .iter_mut()
            .filter(move |event| {
                event.owner == id
                    && !contacts.iter().any(|tracked| {
                        tracked.contact.id == event.contact.id
                            && tracked.target.is_some_and(|target| Self::ancestor(hits, id, target))
                    })
            })
            .map(|event| {
                event.carry = false;
                event.contact
            })
            .chain(contacts.iter().filter_map(move |tracked| {
                let routed = tracked.target.is_some_and(|target| Self::ancestor(hits, id, target))
                    || (tracked.contact.phase == Phase::Cancelled && tracked.owner == Some(id));
                routed.then_some(Contact {
                    captured: tracked.contact.captured && tracked.owner == Some(id),
                    ..tracked.contact
                })
            }))
    }

    pub fn capture(&mut self, owner: Id, contact: ContactId) -> Option<bool> {
        let index = self.contacts.iter().position(|tracked| tracked.contact.id == contact)?;
        let tracked = self.contacts[index];
        if tracked.contact.phase == Phase::Cancelled
            || !tracked
                .target
                .is_some_and(|target| Self::ancestor(&self.hits, owner, target))
        {
            return None;
        }
        if tracked.owner == Some(owner) && tracked.contact.captured {
            return Some(false);
        }
        self.claim(index, owner, Sense::NONE, false);
        Some(true)
    }

    pub fn release(&mut self, owner: Id, contact: ContactId) -> bool {
        let Some(index) = self.contacts.iter().position(|tracked| {
            tracked.contact.id == contact
                && tracked.owner == Some(owner)
                && tracked.contact.captured
                && !matches!(tracked.contact.phase, Phase::Ended | Phase::Cancelled)
        }) else {
            return false;
        };
        self.cancel_owner(index);
        let tracked = &mut self.contacts[index];
        tracked.owner = None;
        tracked.contact.captured = false;
        tracked.dragging = false;
        tracked.sense = Sense::NONE;
        true
    }

    pub fn response(&mut self, id: Id) -> Interaction<T> {
        let mut response = Interaction::default();
        let mut previous = false;
        let mut drag = None;
        let mut scroll = None;
        let mut dragging = false;
        let mut scrolling = false;
        for tracked in &self.contacts {
            previous |= tracked.previous == Some(id);
            if tracked.owner != Some(id) {
                continue;
            }
            let active = !matches!(tracked.contact.phase, Phase::Ended | Phase::Cancelled);
            response.active |= active;
            if self.click_target(tracked) == Some(id) {
                response.clicked = true;
                self.focused = tracked.sense.contains(Sense::FOCUS).then_some(id);
            }
            if !tracked.dragging {
                continue;
            }
            let motion = (tracked.contact, tracked.sense, tracked.drag_started);
            if tracked.sense.intersects(Sense::DRAG) {
                dragging |= active;
                if drag.is_none() || tracked.changed {
                    drag = Some(motion)
                }
            }
            if matches!(tracked.contact.source, ContactSource::Touch(_)) && tracked.sense.intersects(Sense::SCROLL) {
                scrolling |= active;
                if scroll.is_none() || tracked.changed {
                    scroll = Some(motion)
                }
            }
        }
        for event in &mut self.cancellations {
            if event.owner != id {
                continue;
            }
            event.carry = false;
            previous = true;
            let motion = (event.contact, event.sense, false);
            if event.dragging {
                if event.sense.intersects(Sense::DRAG) && drag.is_none() {
                    drag = Some(motion)
                }
                if matches!(event.contact.source, ContactSource::Touch(_))
                    && event.sense.intersects(Sense::SCROLL)
                    && scroll.is_none()
                {
                    scroll = Some(motion)
                }
            }
        }
        response.activated = response.active && !previous;
        response.deactivated = previous && !response.active;
        response.hovered = self.hovered.contains(&id);
        let motion = |contact: Contact<T>, started: bool, continuing: bool| {
            let phase = if matches!(contact.phase, Phase::Ended | Phase::Cancelled) {
                if continuing { Phase::Changed } else { contact.phase }
            } else if started {
                Phase::Started
            } else {
                Phase::Changed
            };
            let delta = if started {
                Point::new(
                    contact.position.x - contact.origin.x,
                    contact.position.y - contact.origin.y,
                )
            } else {
                contact.delta
            };
            (phase, delta)
        };
        if let Some((contact, _, started)) = drag {
            let (phase, delta) = motion(contact, started, dragging);
            response.drag = Some(Drag {
                phase,
                position: contact.position,
                delta,
            });
        }
        if let Some((contact, sense, started)) = scroll {
            let (phase, delta) = motion(contact, started, scrolling);
            response.scroll = Some(Scroll {
                phase,
                delta: Point::new(
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
                continuous: true,
            });
        }
        if let Some(wheel) = &self.wheel {
            if wheel.owner == Some(id) {
                response.scroll = wheel.event.or(response.scroll)
            } else if wheel.cancelled == Some(id) {
                response.scroll = Some(Scroll {
                    phase: Phase::Cancelled,
                    delta: Point::ZERO,
                    continuous: true,
                });
            }
        }
        response
    }

    pub fn begin(&mut self, input: &Input<T>, time: Duration) -> bool {
        // unread cancellation survives one more build
        self.cancellations
            .retain_mut(|event| std::mem::replace(&mut event.carry, false));
        self.contacts
            .retain(|tracked| !matches!(tracked.contact.phase, Phase::Ended | Phase::Cancelled));
        for index in 0..self.contacts.len() {
            let tracked = self.contacts[index];
            let target = tracked
                .target
                .filter(|target| self.exists(*target))
                .or_else(|| tracked.owner.filter(|owner| self.exists(*owner)));
            let tracked = &mut self.contacts[index];
            tracked.previous = tracked.owner;
            tracked.changed = false;
            tracked.drag_started = false;
            tracked.contact.delta = Point::ZERO;
            tracked.contact.phase = if target.is_some() {
                Phase::Changed
            } else {
                Phase::Cancelled
            };
            if target.is_none() {
                tracked.contact.captured = false;
            }
            tracked.target = target;
        }
        if let Some(wheel) = &mut self.wheel {
            if wheel.active {
                wheel.event = None;
                wheel.cancelled = None
            } else {
                self.wheel = None
            }
        }
        let missing = self
            .wheel
            .and_then(|wheel| wheel.owner)
            .is_some_and(|owner| !self.exists(owner));
        if missing {
            let wheel = self.wheel.as_mut().unwrap();
            wheel.cancelled = wheel.owner.take();
        }
        match *input {
            Input::PointerDown {
                position,
                button,
                modifiers,
            } => {
                self.position = Some(position);
                for index in 0..self.contacts.len() {
                    if matches!(self.contacts[index].contact.source, ContactSource::Mouse(_)) {
                        self.contacts[index].contact.modifiers = modifiers;
                        self.update(index, position, time, Phase::Changed);
                    }
                }
                self.press(ContactSource::Mouse(button), position, modifiers, time);
            }
            Input::Touch {
                id,
                position,
                phase: TouchPhase::Down,
            } => {
                self.press(ContactSource::Touch(id), position, Modifiers::NONE, time);
            }
            Input::PointerMove { position, modifiers }
            | Input::PointerUp {
                position, modifiers, ..
            } => {
                self.position = Some(position);
                for index in 0..self.contacts.len() {
                    let ContactSource::Mouse(button) = self.contacts[index].contact.source else {
                        continue;
                    };
                    let released = matches!(input, Input::PointerUp { button: released, .. } if *released == button);
                    self.contacts[index].contact.modifiers = modifiers;
                    self.update(
                        index,
                        position,
                        time,
                        if released { Phase::Ended } else { Phase::Changed },
                    );
                }
            }
            Input::Touch { id, position, phase } => {
                if let Some(index) = self
                    .contacts
                    .iter()
                    .position(|tracked| tracked.contact.source == ContactSource::Touch(id))
                {
                    self.update(
                        index,
                        position,
                        time,
                        match phase {
                            TouchPhase::Move => Phase::Changed,
                            TouchPhase::Up => Phase::Ended,
                            TouchPhase::Cancel => Phase::Cancelled,
                            TouchPhase::Down => unreachable!(),
                        },
                    );
                    if phase == TouchPhase::Up && !self.contacts[index].moved && self.contacts[index].owner.is_none() {
                        self.focused = None;
                    }
                }
            }
            Input::PointerLeave => self.position = None,
            Input::Scroll {
                position,
                delta_x,
                delta_y,
                phase,
                continuous,
                ..
            } => {
                self.position = Some(position);
                let axis = if delta_x == 0.0 && delta_y == 0.0 {
                    Sense::SCROLL
                } else if delta_x.abs() > delta_y.abs() {
                    Sense::SCROLL_X
                } else {
                    Sense::SCROLL_Y
                };
                let previous = self.wheel.filter(|wheel| wheel.active);
                let continuing = continuous && phase != ScrollPhase::Started && previous.is_some();
                let owner = if continuing {
                    previous.and_then(|wheel| wheel.owner)
                } else {
                    self.hit(position)
                        .and_then(|hit| self.target(hit.id, position, axis))
                        .map(|hit| hit.id)
                };
                let cancelled = self
                    .wheel
                    .and_then(|wheel| wheel.cancelled)
                    .or_else(|| previous.filter(|_| !continuing).and_then(|wheel| wheel.owner));
                self.wheel = Some(Wheel {
                    owner,
                    active: continuous && matches!(phase, ScrollPhase::Started | ScrollPhase::Moved),
                    event: owner.map(|_| Scroll {
                        phase: match phase {
                            ScrollPhase::Started => Phase::Started,
                            ScrollPhase::Moved => Phase::Changed,
                            ScrollPhase::Ended => Phase::Ended,
                            ScrollPhase::Cancelled => Phase::Cancelled,
                        },
                        delta: Point::new(delta_x, delta_y),
                        continuous,
                    }),
                    cancelled: cancelled.filter(|previous| Some(*previous) != owner),
                });
            }
            Input::Cancel => {
                self.position = None;
                for tracked in &mut self.contacts {
                    tracked.contact.phase = Phase::Cancelled;
                    tracked.contact.captured = false;
                    tracked.contact.updated = time;
                    tracked.changed = true;
                }
                if let Some(wheel) = &mut self.wheel {
                    wheel.active = false;
                    wheel.event = Some(Scroll {
                        phase: Phase::Cancelled,
                        delta: Point::ZERO,
                        continuous: true,
                    });
                }
            }
            Input::None | Input::Text(_) | Input::Key(_) => {}
        }
        self.hovered.clear();
        if let Some(hit) = self.position.and_then(|position| self.hit(position)) {
            self.hovered.extend(
                self.hits
                    .iter()
                    .filter(|item| item.node <= hit.node && hit.node <= item.end)
                    .map(|item| item.id),
            );
        }
        self.contacts
            .iter()
            .any(|tracked| tracked.changed || tracked.contact.phase == Phase::Cancelled)
            || self.wheel.is_some_and(|wheel| wheel.event.is_some())
            || !self.cancellations.is_empty()
    }

    pub fn end(&mut self, input: &Input<T>) -> bool {
        if self.focused.is_some_and(|id| !self.exists(id)) {
            self.focused = None
        }
        if matches!(input, Input::PointerUp { leave: true, .. }) {
            self.position = None
        }
        let hit = self.position.and_then(|position| self.hit(position));
        let mut count = 0;
        let changed = self.hits.iter().any(|item| {
            if hit.is_some_and(|hit| item.node <= hit.node && hit.node <= item.end) {
                count += 1;
                !self.hovered.contains(&item.id)
            } else {
                false
            }
        });
        changed
            || count != self.hovered.len()
            || self
                .contacts
                .iter()
                .any(|tracked| tracked.target.is_some_and(|target| !self.exists(target)))
            || self
                .wheel
                .is_some_and(|wheel| wheel.active && wheel.owner.is_some_and(|owner| !self.exists(owner)))
            || self
                .cancellations
                .iter()
                .any(|event| event.carry && self.exists(event.owner))
    }

    pub fn focused(&self) -> Option<Id> {
        self.focused
    }
    pub fn is_focused(&self, id: Id) -> bool {
        self.focused == Some(id)
    }
    pub fn focus(&mut self, id: Id) -> bool {
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

    fn press(&mut self, source: ContactSource, position: Point<T>, modifiers: Modifiers, time: Duration) {
        if let Some(index) = self
            .contacts
            .iter()
            .position(|tracked| tracked.contact.source == source)
        {
            self.cancel_owner(index);
            self.contacts.remove(index);
        }
        let target = self.hit(position).map(|hit| hit.id);
        let ordinary = !matches!(source, ContactSource::Mouse(button) if button != PointerButton::Primary);
        let owner = target
            .filter(|_| ordinary)
            .and_then(|target| self.target(target, position, Sense::CLICK));
        if source == ContactSource::Mouse(PointerButton::Primary) {
            self.focused = owner.filter(|hit| hit.sense.contains(Sense::FOCUS)).map(|hit| hit.id);
        }
        self.next_contact = self.next_contact.checked_add(1).expect("contact ids exhausted");
        self.contacts.push(TrackedContact {
            contact: Contact {
                id: ContactId(self.next_contact),
                source,
                phase: Phase::Started,
                origin: position,
                position,
                delta: Point::ZERO,
                started: time,
                updated: time,
                modifiers,
                captured: false,
            },
            target,
            owner: owner.map(|hit| hit.id),
            previous: None,
            sense: owner.map_or(Sense::NONE, |hit| hit.sense),
            moved: false,
            dragging: false,
            drag_started: false,
            changed: true,
        });
        if ordinary
            && owner.is_none()
            && let Some(hit) = target
                .and_then(|target| self.target(target, position, Sense::DRAG))
                .filter(|hit| matches!(source, ContactSource::Mouse(_)) || hit.sense.contains(Sense::DRAG))
        {
            self.claim(self.contacts.len() - 1, hit.id, hit.sense, true)
        }
    }

    fn update(&mut self, index: usize, position: Point<T>, time: Duration, phase: Phase) {
        let tracked = &mut self.contacts[index];
        if matches!(tracked.contact.phase, Phase::Ended | Phase::Cancelled) {
            return;
        }
        tracked.contact.delta = if phase == Phase::Cancelled {
            Point::ZERO
        } else {
            Point::new(
                position.x - tracked.contact.position.x,
                position.y - tracked.contact.position.y,
            )
        };
        tracked.contact.position = position;
        tracked.contact.phase = phase;
        tracked.contact.updated = time;
        tracked.changed = true;
        if phase == Phase::Cancelled {
            tracked.contact.captured = false;
            return;
        }
        let x = (position.x - tracked.contact.origin.x).to_f32();
        let y = (position.y - tracked.contact.origin.y).to_f32();
        if tracked.moved || x * x + y * y < self.config.drag_threshold * self.config.drag_threshold {
            return;
        }
        tracked.moved = true;
        if matches!(tracked.contact.source, ContactSource::Mouse(button) if button != PointerButton::Primary) {
            return;
        }
        let mut sense = if x.abs() >= y.abs() {
            Sense::DRAG_X
        } else {
            Sense::DRAG_Y
        };
        if matches!(tracked.contact.source, ContactSource::Touch(_)) {
            sense |= if x.abs() >= y.abs() {
                Sense::SCROLL_X
            } else {
                Sense::SCROLL_Y
            };
        }
        let target = tracked.target;
        let origin = tracked.contact.origin;
        if let Some(hit) = target.and_then(|target| self.target(target, origin, sense)) {
            self.claim(index, hit.id, hit.sense, true);
        } else {
            self.cancel_owner(index);
            self.contacts[index].owner = None;
        }
    }

    fn claim(&mut self, index: usize, owner: Id, sense: Sense, dragging: bool) {
        if self.contacts[index].owner != Some(owner) {
            self.cancel_owner(index)
        }
        let tracked = &mut self.contacts[index];
        tracked.target = Some(owner);
        tracked.owner = Some(owner);
        tracked.sense = sense;
        tracked.contact.captured = true;
        tracked.moved = true;
        tracked.dragging = dragging;
        tracked.drag_started = dragging;
    }

    fn cancel_owner(&mut self, index: usize) {
        let tracked = self.contacts[index];
        if let Some(owner) = tracked.owner {
            self.cancellations.push(Cancellation {
                owner,
                contact: Contact {
                    phase: Phase::Cancelled,
                    delta: Point::ZERO,
                    captured: false,
                    ..tracked.contact
                },
                sense: tracked.sense,
                dragging: tracked.dragging,
                carry: true,
            });
        }
    }

    fn click_target(&self, tracked: &TrackedContact<T>) -> Option<Id> {
        let owner = tracked.owner?;
        (tracked.contact.phase == Phase::Ended
            && !tracked.moved
            && tracked.sense.contains(Sense::CLICK)
            && self
                .hit(tracked.contact.position)
                .is_some_and(|hit| Self::ancestor(&self.hits, owner, hit.id)))
        .then_some(owner)
    }

    fn exists(&self, id: Id) -> bool {
        self.hits.iter().any(|hit| hit.id == id)
    }

    fn ancestor(hits: &[HitItem<T>], id: Id, target: Id) -> bool {
        if id == target {
            return true;
        }
        let Some(node) = hits.iter().find(|hit| hit.id == target).map(|hit| hit.node) else {
            return false;
        };
        hits.iter()
            .any(|hit| hit.id == id && hit.node <= node && node <= hit.end)
    }

    fn hit(&self, position: Point<T>) -> Option<HitItem<T>> {
        self.hits.iter().rev().find(|hit| self.contains(hit, position)).copied()
    }

    fn target(&self, target: Id, position: Point<T>, sense: Sense) -> Option<HitItem<T>> {
        let node = self.hits.iter().find(|hit| hit.id == target)?.node;
        self.hits
            .iter()
            .rev()
            .find(|hit| {
                hit.sense.intersects(sense) && hit.node <= node && node <= hit.end && self.contains(hit, position)
            })
            .copied()
    }

    fn contains(&self, hit: &HitItem<T>, position: Point<T>) -> bool {
        if !hit.area.contains(position) {
            return false;
        }
        let mut clip = hit.clip;
        while let Some(index) = clip.index() {
            let stored = &self.clips[index];
            if !stored.shape.contains(position) {
                return false;
            }
            clip = stored.parent;
        }
        true
    }
}

pub fn resolve<C: Context>(frame: &mut Frame<C>) {
    let exists = |id| {
        matches!(frame.requests.get(&id), Some(Request::Interaction(_)))
            && frame.nodes.iter().any(|stored| stored.widget_id == id)
    };
    for index in 0..frame.interaction.contacts.len() {
        let tracked = frame.interaction.contacts[index];
        let Some(target) = tracked.target else { continue };
        if tracked.contact.captured || exists(target) {
            continue;
        }
        let Some(node) = frame
            .interaction
            .hits
            .iter()
            .find(|hit| hit.id == target)
            .map(|hit| hit.node)
        else {
            continue;
        };
        let target = frame
            .interaction
            .hits
            .iter()
            .filter(|hit| hit.node <= node && node <= hit.end && exists(hit.id))
            .max_by_key(|hit| hit.node)
            .map(|hit| hit.id);
        if tracked.owner.is_some_and(|owner| !exists(owner)) {
            frame.interaction.cancel_owner(index);
            frame.interaction.contacts[index].owner = None;
        }
        frame.interaction.contacts[index].target = target;
    }
    frame.geometry_previous.clear();
    frame.interaction.hits.clear();
    if !frame.requests.is_empty() {
        let count = frame.nodes.len();
        let nodes = &frame.nodes[..count];
        let geometry = &frame.node_geometry[..count];
        let mut collect = {
            #[inline(always)]
            |node: usize, stored: &super::StoredNode, area: Rect<C::Scalar>| {
                let id = stored.widget_id;
                let Some(request) = frame.requests.get(&id).copied() else {
                    return;
                };
                frame.geometry_previous.push((id, area));
                let Request::Interaction(sense) = request else { return };
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
                let clip_bounds = stored
                    .resolved_clip
                    .index()
                    .map_or(frame.screen, |clip| frame.interaction.clips[clip].bounds);
                frame.interaction.hits.push(HitItem {
                    id,
                    area: area.intersection(clip_bounds).unwrap_or_default(),
                    clip: stored.resolved_clip,
                    sense,
                    node,
                    end: stored.subtree_end as usize,
                });
            }
        };
        if frame.paint_order.is_empty() {
            for (node, (stored, geometry)) in nodes.iter().zip(geometry).enumerate() {
                collect(node, stored, geometry.area)
            }
        } else {
            for id in &frame.paint_order[..count] {
                let node = id.index();
                collect(node, &nodes[node], geometry[node].area);
            }
        }
    }
    frame.geometry_previous.sort_unstable_by_key(|(id, _)| *id);
    if frame.interaction.end(&frame.input) {
        frame.frame_requested = true
    }
}

#[derive(Clone, Copy)]
struct TrackedContact<T> {
    contact: Contact<T>,
    target: Option<Id>,
    owner: Option<Id>,
    previous: Option<Id>,
    sense: Sense,
    moved: bool,
    dragging: bool,
    drag_started: bool,
    changed: bool,
}

struct Cancellation<T> {
    owner: Id,
    contact: Contact<T>,
    sense: Sense,
    dragging: bool,
    carry: bool,
}

#[derive(Clone, Copy)]
struct Wheel {
    owner: Option<Id>,
    event: Option<Scroll>,
    active: bool,
    cancelled: Option<Id>,
}

#[derive(Clone, Copy)]
struct HitItem<T> {
    id: Id,
    area: Rect<T>,
    clip: ResolvedClipId,
    sense: Sense,
    node: usize,
    end: usize,
}
