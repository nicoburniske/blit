use std::{any::TypeId, mem::MaybeUninit, num::NonZeroU32, time::Duration};

use super::{Frame, Request, ResolvedClip, ResolvedClipId};
use crate::{
    Contact, ContactId, ContactSource, Context, Gesture, Id, Input, Interaction, Modifiers, Phase, PointerButton,
    Scalar, Scroll, ScrollPhase, Sense, TouchPhase,
    geometry::{Point, Rect, Sides},
    internal::bump::Bump,
};

// movement after which a press is no longer a click
const CLICK_SLOP: f32 = 6.0;

// slot node for a named widget missing from the layout
const UNRESOLVED: u32 = u32::MAX;

/// contacts routed to one gesture before a build
pub struct GestureCx<'a, T> {
    id: Id,
    node: u32,
    end: u32,
    contacts: &'a mut [TrackedContact<T>],
    time: Duration,
    deadline: &'a mut Option<Duration>,
}

impl<T: Scalar> GestureCx<'_, T> {
    pub fn time(&self) -> Duration {
        self.time
    }

    /// pending contacts within this widget and contacts it captured
    pub fn contacts(&self) -> Contacts<'_, T> {
        Contacts {
            contacts: self.contacts.iter(),
            id: self.id,
            node: self.node,
            end: self.end,
        }
    }

    /// takes a pending contact within this widget from its ancestors and click until it ends
    pub fn capture(&mut self, contact: ContactId) {
        let (node, end) = (self.node as usize, self.end as usize);
        if let Some(tracked) = self.contacts.iter_mut().find(|tracked| tracked.id == contact)
            && let Route::Pending {
                target: Some(target), ..
            } = tracked.route
            && node <= target.node
            && target.node <= end
        {
            tracked.route = Route::Captured {
                owner: self.id,
                click: false,
            };
        }
    }

    /// builds a frame at this time without input
    pub fn wake_at(&mut self, time: Duration) {
        *self.deadline = Some(self.deadline.map_or(time, |deadline| deadline.min(time)));
    }
}

/// contacts visible to one gesture
pub struct Contacts<'a, T> {
    contacts: std::slice::Iter<'a, TrackedContact<T>>,
    id: Id,
    node: u32,
    end: u32,
}

impl<T: Scalar> Contacts<'_, T> {
    /// takes the next `N` contacts when at least `N` remain
    pub fn next_array<const N: usize>(&mut self) -> Option<[Contact<T>; N]> {
        let mut contacts = [const { MaybeUninit::uninit() }; N];
        for contact in &mut contacts {
            // contacts are copy so an early return leaks nothing
            contact.write(self.next()?);
        }
        // safety: every element was written above
        Some(unsafe { contacts.as_ptr().cast::<[Contact<T>; N]>().read() })
    }
}

impl<T: Scalar> Iterator for Contacts<'_, T> {
    type Item = Contact<T>;

    fn next(&mut self) -> Option<Contact<T>> {
        let tracked = self.contacts.find(|tracked| match tracked.route {
            Route::Pending {
                target: Some(target), ..
            } => self.node as usize <= target.node && target.node <= self.end as usize,
            Route::Captured { owner, .. } => owner == self.id,
            Route::Pending { target: None, .. } | Route::Retired => false,
        })?;
        Some(Contact {
            id: tracked.id,
            source: match tracked.source {
                Source::Mouse(button) => ContactSource::Mouse(button),
                Source::Touch(_) => ContactSource::Touch,
            },
            phase: tracked.phase,
            origin: tracked.origin,
            position: tracked.position,
            delta: tracked.delta,
            started: tracked.started,
            modifiers: tracked.modifiers,
            captured: matches!(tracked.route, Route::Captured { .. }),
        })
    }
}

#[derive(Default)]
pub struct InteractionState<T> {
    pub clips: Vec<ResolvedClip<T>>,
    pub deadline: Option<Duration>,
    pub focused: Option<Id>,
    pub position: Option<Point<T>>,
    contacts: Vec<TrackedContact<T>>,
    next_contact: u32,
    active_previous: Vec<Id>,
    wheel: Option<Wheel>,
    hits: Vec<HitItem<T>>,
    // preorder by node so reverse iteration visits descendants first
    gestures: Vec<GestureSlot>,
    // slots requested by this build are moved before the cursor
    cursor: usize,
    gesture_kinds: Vec<GestureKind<T>>,
    // cells of every gesture kind, reused through each kind's free list
    gesture_state: Bump,
    key_focus: Option<Id>,
    hovered: Vec<Id>,
}

impl<T: Scalar> InteractionState<T> {
    pub fn gesture<G: Gesture<T>>(&mut self, id: Id, node: Option<usize>, initial: G) -> G::Output {
        let type_id = TypeId::of::<G>();
        let kind = self
            .gesture_kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                self.gesture_kinds.push(GestureKind {
                    type_id,
                    update: |state, offset, cx| {
                        // safety: slots of this kind point at live cells holding G and its output
                        let (gesture, output) =
                            unsafe { &mut *state.as_ptr().add(offset as usize).cast::<(G, G::Output)>() };
                        *output = gesture.update(cx);
                    },
                    free: None,
                });
                self.gesture_kinds.len() - 1
            });
        let kind = u16::try_from(kind).expect("too many gesture kinds");
        let matches = |slot: &GestureSlot| slot.id == id && slot.kind == kind;
        let cursor = self.cursor;
        // builds request gestures in nearly the same order each frame
        let index = if let Some(offset) = self.gestures[cursor..].iter().position(matches) {
            self.gestures.swap(cursor, cursor + offset);
            self.cursor += 1;
            cursor
        } else if let Some(index) = self.gestures[..cursor].iter().position(matches) {
            index
        } else {
            let value = (initial, G::Output::default());
            // reserve first so a panic cannot lose a reused cell
            self.gestures.reserve(1);
            let free = &mut self.gesture_kinds[kind as usize].free;
            let offset = match *free {
                Some(offset) => {
                    let cell = self
                        .gesture_state
                        .as_ptr()
                        .wrapping_add(offset as usize)
                        .cast::<GestureCell<(G, G::Output)>>();
                    // safety: free cells of this kind are dead cells of this layout holding the next link
                    unsafe {
                        *free = (*cell).next;
                        cell.write(GestureCell { value });
                    }
                    offset
                }
                None => self.gesture_state.push(GestureCell { value }).byte(),
            };
            self.gestures.push(GestureSlot {
                id,
                node: UNRESOLVED,
                end: 0,
                offset,
                kind,
            });
            let last = self.gestures.len() - 1;
            self.gestures.swap(cursor, last);
            self.cursor += 1;
            cursor
        };
        let slot = &mut self.gestures[index];
        slot.node = node.map_or(UNRESOLVED, |node| u32::try_from(node).expect("too many frame nodes"));
        // safety: slots of this kind point at live cells holding G and its output
        unsafe {
            (*self
                .gesture_state
                .as_ptr()
                .add(slot.offset as usize)
                .cast::<(G, G::Output)>())
            .1
        }
    }

    pub fn response(&mut self, id: Id) -> Interaction {
        let mut response = Interaction {
            hovered: self.position.is_some() && self.hovered.contains(&id),
            ..Interaction::default()
        };
        for index in 0..self.contacts.len() {
            let tracked = self.contacts[index];
            if let Route::Pending { click: Some(click), .. } = tracked.route
                && click == id
                && tracked.phase == Phase::Ended
                && self
                    .hit(tracked.position)
                    .is_some_and(|hit| Self::ancestor(&self.hits, id, hit.id))
            {
                // the first read commits the click so repeated reads agree
                self.contacts[index].route = Route::Captured { owner: id, click: true };
            }
            let route = self.contacts[index].route;
            if route.receiver() == Some(id) {
                response.active |= !matches!(tracked.phase, Phase::Ended | Phase::Cancelled);
                response.clicked |=
                    tracked.phase == Phase::Ended && matches!(route, Route::Captured { click: true, .. });
            }
        }
        let previous = self.active_previous.contains(&id);
        response.activated = response.active && !previous;
        response.deactivated = previous && !response.active;
        if let Some(wheel) = &self.wheel {
            if wheel.owner == Some(id) {
                response.scroll = wheel.event.or(response.scroll);
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

    pub fn begin(&mut self, input: &Input<T>, time: Duration) {
        self.key_focus = self.focused;
        self.contacts
            .retain(|tracked| !matches!(tracked.phase, Phase::Ended | Phase::Cancelled));
        for tracked in &mut self.contacts {
            tracked.phase = Phase::Held;
            tracked.delta = Point::ZERO;
        }
        if let Some(wheel) = &mut self.wheel {
            if wheel.active {
                wheel.event = None;
                wheel.cancelled = None;
            } else {
                self.wheel = None;
            }
        }
        match *input {
            Input::PointerDown {
                position,
                button,
                modifiers,
            } => {
                self.position = Some(position);
                for index in 0..self.contacts.len() {
                    if matches!(self.contacts[index].source, Source::Mouse(_)) {
                        self.contacts[index].modifiers = modifiers;
                        self.update(index, position, Phase::Changed);
                    }
                }
                self.press(Source::Mouse(button), position, modifiers, time);
            }
            Input::PointerMove { position, modifiers }
            | Input::PointerUp {
                position, modifiers, ..
            } => {
                self.position = Some(position);
                for index in 0..self.contacts.len() {
                    let Source::Mouse(button) = self.contacts[index].source else {
                        continue;
                    };
                    let released = matches!(input, Input::PointerUp { button: released, .. } if *released == button);
                    self.contacts[index].modifiers = modifiers;
                    self.update(index, position, if released { Phase::Ended } else { Phase::Changed });
                }
                if matches!(input, Input::PointerUp { leave: true, .. }) {
                    self.position = None;
                }
            }
            Input::Touch {
                id,
                position,
                phase: TouchPhase::Down,
            } => {
                self.press(Source::Touch(id), position, Modifiers::NONE, time);
            }
            Input::Touch { id, position, phase } => {
                if let Some(index) = self
                    .contacts
                    .iter()
                    .position(|tracked| tracked.source == Source::Touch(id))
                {
                    self.update(
                        index,
                        position,
                        match phase {
                            TouchPhase::Move => Phase::Changed,
                            TouchPhase::Up => Phase::Ended,
                            TouchPhase::Cancel => Phase::Cancelled,
                            TouchPhase::Down => unreachable!(),
                        },
                    );
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
                } else if continuous && matches!(phase, ScrollPhase::Ended | ScrollPhase::Cancelled) {
                    None
                } else {
                    self.hit(position)
                        .and_then(|hit| self.target(hit.node, position, axis))
                        .map(|hit| hit.id)
                };
                let cancelled = previous.filter(|_| !continuing).and_then(|wheel| wheel.owner);
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
                        delta: if phase == ScrollPhase::Cancelled {
                            Point::ZERO
                        } else {
                            Point::new(delta_x, delta_y)
                        },
                        continuous,
                    }),
                    cancelled: cancelled.filter(|previous| Some(*previous) != owner),
                });
            }
            Input::Cancel => {
                self.position = None;
                for index in 0..self.contacts.len() {
                    self.update(index, self.contacts[index].position, Phase::Cancelled);
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
        // deepest gestures claim contacts first
        self.deadline = None;
        self.cursor = 0;
        for slot in self.gestures.iter().rev() {
            (self.gesture_kinds[slot.kind as usize].update)(
                &self.gesture_state,
                slot.offset,
                &mut GestureCx {
                    id: slot.id,
                    node: slot.node,
                    end: slot.end,
                    contacts: &mut self.contacts,
                    time,
                    deadline: &mut self.deadline,
                },
            );
        }
        if let Input::Touch {
            id,
            position,
            phase: TouchPhase::Up,
        } = *input
            && let Some(tracked) = self.contacts.iter().find(|tracked| tracked.source == Source::Touch(id))
            && let Route::Pending { click, tap: true, .. } = tracked.route
        {
            let target = click.filter(|owner| {
                self.hit(position)
                    .is_some_and(|hit| Self::ancestor(&self.hits, *owner, hit.id))
            });
            self.focused = target.filter(|owner| {
                self.hits
                    .iter()
                    .any(|hit| hit.id == *owner && hit.sense.contains(Sense::FOCUS))
            });
        }
        if !matches!(input, Input::PointerLeave) {
            self.hovered.clear();
            if let Some(hit) = self.position.and_then(|position| self.hit(position)) {
                self.hovered.extend(
                    self.hits
                        .iter()
                        .filter(|item| item.node <= hit.node && hit.node <= item.end)
                        .map(|item| item.id),
                );
            }
        }
    }

    pub fn routes(&self, id: Id, root: Id, input: &Input<T>) -> bool {
        match *input {
            Input::Key(_) | Input::Text(_) => {
                id == root
                    || self
                        .key_focus
                        .is_some_and(|owner| Self::ancestor(&self.hits, id, owner))
            }
            Input::PointerDown { button, .. } | Input::PointerUp { button, .. } => {
                let phase = if matches!(input, Input::PointerDown { .. }) {
                    Phase::Started
                } else {
                    Phase::Ended
                };
                self.contacts.iter().any(|tracked| {
                    tracked.source == Source::Mouse(button)
                        && tracked.phase == phase
                        && Self::visible(&self.hits, id, tracked.route)
                })
            }
            Input::PointerMove { .. } => {
                if self.contacts.iter().any(|tracked| {
                    matches!(tracked.source, Source::Mouse(_))
                        && !matches!(tracked.phase, Phase::Ended | Phase::Cancelled)
                }) {
                    self.contacts.iter().any(|tracked| {
                        matches!(tracked.source, Source::Mouse(_))
                            && tracked.phase == Phase::Changed
                            && Self::visible(&self.hits, id, tracked.route)
                    })
                } else {
                    self.hovered.contains(&id)
                }
            }
            Input::PointerLeave => self.hovered.contains(&id),
            Input::Touch { id: source, phase, .. } => {
                let phase = match phase {
                    TouchPhase::Down => Phase::Started,
                    TouchPhase::Move => Phase::Changed,
                    TouchPhase::Up => Phase::Ended,
                    TouchPhase::Cancel => Phase::Cancelled,
                };
                self.contacts.iter().any(|tracked| {
                    tracked.source == Source::Touch(source)
                        && tracked.phase == phase
                        && Self::visible(&self.hits, id, tracked.route)
                })
            }
            Input::Scroll { .. } => self
                .wheel
                .and_then(|wheel| wheel.owner)
                .is_some_and(|owner| Self::ancestor(&self.hits, id, owner)),
            Input::Cancel => {
                self.contacts
                    .iter()
                    .any(|tracked| Self::visible(&self.hits, id, tracked.route))
                    || self
                        .wheel
                        .and_then(|wheel| wheel.owner)
                        .is_some_and(|owner| Self::ancestor(&self.hits, id, owner))
            }
            Input::None => false,
        }
    }

    fn press(&mut self, source: Source, position: Point<T>, modifiers: Modifiers, time: Duration) {
        for index in 0..self.contacts.len() {
            if self.contacts[index].source == source {
                self.update(index, self.contacts[index].position, Phase::Cancelled);
            }
        }
        let target = self.hit(position);
        let ordinary = !matches!(source, Source::Mouse(button) if button != PointerButton::Primary);
        let click = target
            .filter(|_| ordinary)
            .and_then(|hit| self.target(hit.node, position, Sense::CLICK));
        if source == Source::Mouse(PointerButton::Primary) {
            self.focused = click.filter(|hit| hit.sense.contains(Sense::FOCUS)).map(|hit| hit.id);
        }
        let busy = click.is_some_and(|hit| self.busy(hit.id));
        let route = Route::Pending {
            target: target.map(|hit| Target {
                id: hit.id,
                node: hit.node,
            }),
            click: click.filter(|_| !busy).map(|hit| hit.id),
            tap: ordinary && !busy,
        };
        // ids wrap past zero, only live contacts need distinct ids
        let id = NonZeroU32::new(self.next_contact.wrapping_add(1)).unwrap_or(NonZeroU32::MIN);
        self.next_contact = id.get();
        self.contacts.push(TrackedContact {
            id: ContactId(id),
            source,
            route,
            phase: Phase::Started,
            origin: position,
            position,
            delta: Point::ZERO,
            started: time,
            modifiers,
        });
    }

    fn update(&mut self, index: usize, position: Point<T>, phase: Phase) {
        let tracked = &mut self.contacts[index];
        if matches!(tracked.phase, Phase::Ended | Phase::Cancelled) {
            return;
        }
        tracked.delta = if phase == Phase::Cancelled {
            Point::ZERO
        } else {
            Point::new(position.x - tracked.position.x, position.y - tracked.position.y)
        };
        tracked.position = position;
        tracked.phase = phase;
        let x = (position.x - tracked.origin.x).to_f32();
        let y = (position.y - tracked.origin.y).to_f32();
        let distance = x * x + y * y;
        if let Route::Pending { click, tap, .. } = &mut tracked.route
            && distance >= CLICK_SLOP * CLICK_SLOP
        {
            *click = None;
            *tap = false;
        }
    }

    /// whether a live contact already drives this widget
    fn busy(&self, id: Id) -> bool {
        self.contacts.iter().any(|tracked| {
            !matches!(tracked.phase, Phase::Ended | Phase::Cancelled) && tracked.route.receiver() == Some(id)
        })
    }

    fn visible(hits: &[HitItem<T>], id: Id, route: Route) -> bool {
        match route {
            Route::Pending {
                target: Some(target), ..
            } => hits
                .iter()
                .any(|hit| hit.id == id && hit.node <= target.node && target.node <= hit.end),
            Route::Captured { owner, .. } => owner == id,
            Route::Pending { target: None, .. } | Route::Retired => false,
        }
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

    fn target(&self, node: usize, position: Point<T>, sense: Sense) -> Option<HitItem<T>> {
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
    let requests = &frame.requests;
    let nodes = &frame.nodes;
    let hits = &frame.interaction.hits;
    let node = |id| {
        matches!(requests.get(&id), Some(Request::Interaction { .. }))
            .then(|| nodes.iter().position(|stored| stored.widget_id == id))
            .flatten()
    };
    let next_route = |route| match route {
        Route::Pending { target, click, tap } => {
            let Some(Target {
                id: target,
                node: previous,
            }) = target
            else {
                return Some(route);
            };
            let target = if node(target).is_some() {
                target
            } else {
                hits.iter()
                    .filter(|hit| hit.node <= previous && previous <= hit.end && node(hit.id).is_some())
                    .max_by_key(|hit| hit.node)?
                    .id
            };
            let target_node = node(target)?;
            let click = click.filter(|id| {
                matches!(requests.get(id), Some(Request::Interaction { sense, .. }) if sense.contains(Sense::CLICK))
                    && node(*id)
                        .is_some_and(|index| index <= target_node && target_node <= nodes[index].subtree_end as usize)
            });
            Some(Route::Pending {
                target: Some(Target {
                    id: target,
                    node: target_node,
                }),
                click,
                tap,
            })
        }
        Route::Captured { owner, .. } => node(owner).map(|_| route),
        Route::Retired => None,
    };
    for (&id, request) in requests {
        if !matches!(request, Request::Interaction { response: true, .. }) {
            continue;
        }
        if frame.interaction.busy(id) {
            let next_active = frame.interaction.contacts.iter().any(|tracked| {
                !matches!(tracked.phase, Phase::Ended | Phase::Cancelled)
                    && next_route(tracked.route).and_then(Route::receiver) == Some(id)
            });
            frame.frame_requested |= !next_active;
        }
    }
    frame.interaction.active_previous.clear();
    for tracked in &mut frame.interaction.contacts {
        let next = next_route(tracked.route);
        if !matches!(tracked.phase, Phase::Ended | Phase::Cancelled)
            && let Some(receiver) = tracked.route.receiver()
        {
            frame.interaction.active_previous.push(receiver);
        }
        tracked.route = next.unwrap_or(Route::Retired);
    }
    if frame.interaction.focused.is_some_and(|id| node(id).is_none()) {
        frame.interaction.focused = None;
    }
    if let Some(wheel) = &mut frame.interaction.wheel
        && wheel.owner.is_some_and(|id| node(id).is_none())
    {
        wheel.owner = None;
        wheel.event = None;
    }
    frame.geometry_previous.clear();
    frame.interaction.hits.clear();
    if !frame.requests.is_empty() {
        let count = frame.nodes.len();
        let geometry = &frame.node_geometry[..count];
        let mut collect = {
            #[inline(always)]
            |index: usize, stored: &super::StoredNode, area: Rect<C::Scalar>| {
                let id = stored.widget_id;
                let Some(request) = frame.requests.get(&id).copied() else {
                    return;
                };
                frame.geometry_previous.push((id, area));
                let Request::Interaction { sense, .. } = request else {
                    return;
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
                let clip_bounds = stored
                    .resolved_clip
                    .index()
                    .map_or(frame.screen, |clip| frame.interaction.clips[clip].bounds);
                frame.interaction.hits.push(HitItem {
                    id,
                    area: area.intersection(clip_bounds).unwrap_or_default(),
                    clip: stored.resolved_clip,
                    sense,
                    node: index,
                    end: stored.subtree_end as usize,
                });
            }
        };
        if frame.paint_order.is_empty() {
            for (index, (stored, geometry)) in nodes.iter().zip(geometry).enumerate() {
                collect(index, stored, geometry.area);
            }
        } else {
            for id in &frame.paint_order[..count] {
                collect(id.index(), &nodes[id.index()], geometry[id.index()].area);
            }
        }
    }
    frame.geometry_previous.sort_unstable_by_key(|(id, _)| *id);
    let interaction = &mut frame.interaction;
    for slot in &interaction.gestures[interaction.cursor..] {
        let free = &mut interaction.gesture_kinds[slot.kind as usize].free;
        let cell = interaction.gesture_state.as_ptr().wrapping_add(slot.offset as usize);
        // safety: unrequested cells are dead and every cell starts with room for the link
        unsafe { cell.cast::<Option<u32>>().write(*free) };
        *free = Some(slot.offset);
    }
    interaction.gestures.truncate(interaction.cursor);
    for slot in &mut interaction.gestures {
        if slot.node == UNRESOLVED
            && let Some(index) = nodes.iter().position(|stored| stored.widget_id == slot.id)
        {
            slot.node = index as u32;
        }
        slot.end = nodes.get(slot.node as usize).map_or(0, |stored| stored.subtree_end);
    }
    // stable insertion sort stays near linear since builds request in preorder
    let gestures = &mut interaction.gestures;
    for sorted in 1..gestures.len() {
        let mut index = sorted;
        while index > 0 && gestures[index - 1].node > gestures[index].node {
            gestures.swap(index - 1, index);
            index -= 1;
        }
    }
    let hit = frame
        .interaction
        .position
        .and_then(|position| frame.interaction.hit(position));
    let previous = if frame.interaction.position.is_some() {
        frame
            .interaction
            .hovered
            .iter()
            .filter(|id| {
                matches!(
                    frame.requests.get(id),
                    Some(Request::Interaction { response: true, .. })
                )
            })
            .count()
    } else {
        0
    };
    let mut count = 0;
    for item in &frame.interaction.hits {
        if hit.is_some_and(|hit| item.node <= hit.node && hit.node <= item.end)
            && matches!(
                frame.requests.get(&item.id),
                Some(Request::Interaction { response: true, .. })
            )
        {
            count += 1;
            frame.frame_requested |= !frame.interaction.hovered.contains(&item.id);
        }
    }
    frame.frame_requested |= previous != count;
}

#[derive(Clone, Copy)]
enum Route {
    Pending {
        target: Option<Target>,
        click: Option<Id>,
        // still within the threshold of its press
        tap: bool,
    },
    Captured {
        owner: Id,
        click: bool,
    },
    Retired,
}

impl Route {
    /// the widget whose response this contact drives
    fn receiver(self) -> Option<Id> {
        match self {
            Route::Pending { click, .. } => click,
            Route::Captured { owner, .. } => Some(owner),
            Route::Retired => None,
        }
    }
}

/// a pending contact's deepest widget and its node in the previous layout
#[derive(Clone, Copy)]
struct Target {
    id: Id,
    node: usize,
}

#[derive(Clone, Copy)]
struct TrackedContact<T> {
    id: ContactId,
    source: Source,
    route: Route,
    phase: Phase,
    origin: Point<T>,
    position: Point<T>,
    delta: Point<T>,
    started: Duration,
    modifiers: Modifiers,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Mouse(PointerButton),
    Touch(u64),
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

#[derive(Clone, Copy)]
struct GestureSlot {
    id: Id,
    // node and subtree end in the previous layout
    node: u32,
    end: u32,
    offset: u32,
    kind: u16,
}

struct GestureKind<T> {
    type_id: TypeId,
    update: fn(&Bump, u32, &mut GestureCx<'_, T>),
    // dead cells of this kind linked through their first bytes
    free: Option<u32>,
}

/// a gesture and its latest output, which holds the free link once it is dead
#[derive(Clone, Copy)]
#[repr(C)]
union GestureCell<S: Copy> {
    value: S,
    next: Option<u32>,
}
