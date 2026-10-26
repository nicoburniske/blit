use std::{
    any::TypeId,
    hash::{Hash, Hasher},
    mem::MaybeUninit,
    num::NonZeroU32,
    time::Duration,
};

use super::{Frame, ResolvedClip, ResolvedClipId};
use crate::{
    Contact, ContactId, ContactSource, Context, Hold, Id, Input, Interact, Modifiers, Phase, PointerButton, Scalar,
    Scroll, ScrollPhase, TouchPhase,
    geometry::{Point, Rect, Sides},
    internal::{bump::Bump, hash::FxHasher},
};

// movement after which a press is no longer a tap
const TAP_SLOP: f32 = 6.0;

// slot node for a named widget missing from the layout
const UNRESOLVED: u32 = u32::MAX;

/// input routed to one interaction kind before a build
pub struct InteractCx<'a, T> {
    id: Id,
    node: u32,
    end: u32,
    state: &'a mut InteractionState<T>,
    input: &'a Input<T>,
    time: Duration,
}

impl<T: Scalar> InteractCx<'_, T> {
    pub fn time(&self) -> Duration {
        self.time
    }

    /// builds a frame at this time without input
    pub fn wake_at(&mut self, time: Duration) {
        let deadline = &mut self.state.deadline;
        *deadline = Some(deadline.map_or(time, |deadline| deadline.min(time)));
    }

    /// pending contacts within this widget and contacts it captured
    pub fn contacts(&self) -> Contacts<'_, T> {
        Contacts {
            contacts: self.state.contacts.iter(),
            id: self.id,
            node: self.node,
            end: self.end,
        }
    }

    /// holds a free pending contact within this widget until something captures it
    pub fn claim(&mut self, contact: ContactId) {
        let id = self.id;
        if let Some(Route::Pending { claim, .. }) = self.pending(contact)
            && claim.is_none()
        {
            *claim = Some(id);
        }
    }

    /// takes a pending contact within this widget from every other widget until it ends
    pub fn capture(&mut self, contact: ContactId) {
        let owner = self.id;
        let Some(route) = self.pending(contact) else {
            return;
        };
        let claimant = match *route {
            Route::Pending { claim, .. } => claim.filter(|claim| *claim != owner),
            _ => None,
        };
        *route = Route::Captured { owner };
        self.state.stolen.extend(claimant);
    }

    pub fn hovered(&self) -> bool {
        self.state.position.is_some() && self.state.hover.is_some_and(|node| self.contains(node))
    }

    /// whether `position` falls on this widget rather than something above it
    pub fn over(&self, position: Point<T>) -> bool {
        self.state.hit(position).is_some_and(|hit| self.contains(hit.node))
    }

    pub fn focused(&self) -> bool {
        self.state.focused == Some(self.id)
    }

    /// focuses this widget unless a deeper widget took focus during this update
    pub fn focus(&mut self) {
        if !self.state.focus_taken {
            self.state.focused = Some(self.id);
            self.state.focus_taken = true;
        }
    }

    /// this frame's input, the same for every widget
    pub fn input(&self) -> Input<T> {
        *self.input
    }

    /// wheel scrolling over this widget, or the cancellation of a sequence it held
    pub fn wheel(&self) -> Option<Scroll> {
        let wheel = self.state.wheel?;
        let event = wheel.event.filter(|_| match wheel.owner {
            Some(owner) => owner == self.id,
            None => wheel.target.is_some_and(|node| self.contains(node)),
        });
        event.or((wheel.cancelled == Some(self.id)).then_some(Scroll {
            phase: Phase::Cancelled,
            delta: Point::ZERO,
            continuous: true,
        }))
    }

    /// keeps a continuous wheel sequence over this widget until it ends
    pub fn capture_wheel(&mut self) {
        let id = self.id;
        let (node, end) = (self.node as usize, self.end as usize);
        if let Some(wheel) = &mut self.state.wheel
            && wheel.owner.is_none()
            && wheel.target.is_some_and(|target| node <= target && target <= end)
        {
            wheel.owner = Some(id);
        }
    }

    fn contains(&self, node: usize) -> bool {
        self.node as usize <= node && node <= self.end as usize
    }

    fn pending(&mut self, contact: ContactId) -> Option<&mut Route> {
        let (node, end) = (self.node as usize, self.end as usize);
        let tracked = self.state.contacts.iter_mut().find(|tracked| tracked.id == contact)?;
        match tracked.route {
            Route::Pending {
                target: Some(target), ..
            } if node <= target.node && target.node <= end => Some(&mut tracked.route),
            _ => None,
        }
    }
}

/// contacts visible to one interaction kind
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
        let (id, node, end) = (self.id, self.node as usize, self.end as usize);
        let (tracked, hold) = self.contacts.find_map(|tracked| {
            let hold = match tracked.route {
                Route::Pending {
                    target: Some(target),
                    claim,
                } if node <= target.node && target.node <= end => match claim {
                    None => Hold::Free,
                    Some(claim) if claim == id => Hold::Claimed,
                    Some(_) => Hold::ClaimedElsewhere,
                },
                Route::Captured { owner } if owner == id => Hold::Captured,
                _ => return None,
            };
            Some((tracked, hold))
        })?;
        Some(Contact {
            id: tracked.id,
            source: match tracked.source {
                Source::Mouse(button) => ContactSource::Mouse(button),
                Source::Touch(_) => ContactSource::Touch,
            },
            phase: tracked.phase,
            hold,
            origin: tracked.origin,
            position: tracked.position,
            delta: tracked.delta,
            started: tracked.started,
            modifiers: tracked.modifiers,
            tap: tracked.tap,
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
    wheel: Option<WheelState>,
    hits: Vec<HitItem<T>>,
    // node under the pointer in the previous layout
    hover: Option<usize>,
    // ids hovered when this build began, compared after layout
    hover_key: u64,
    // a deeper widget took focus during this update
    focus_taken: bool,
    // claimants whose contacts an ancestor captured after they updated
    stolen: Vec<Id>,
    kinds: Kinds<T>,
}

impl<T: Scalar> InteractionState<T> {
    pub fn interact<K: Interact<T>>(&mut self, id: Id, node: Option<usize>, initial: K) -> K::Output {
        let kinds = &mut self.kinds;
        let type_id = TypeId::of::<K>();
        let kind = kinds
            .kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                kinds.kinds.push(Kind {
                    type_id,
                    update: |state, offset, cx| {
                        // safety: slots of this kind point at live cells holding K and its output
                        let (kind, output) =
                            unsafe { &mut *state.as_ptr().add(offset as usize).cast::<(K, K::Output)>() };
                        *output = kind.update(cx);
                    },
                    free: None,
                });
                kinds.kinds.len() - 1
            });
        let kind = u16::try_from(kind).expect("too many interaction kinds");
        let matches = |slot: &Slot| slot.id == id && slot.kind == kind;
        let cursor = kinds.cursor;
        // builds request kinds in nearly the same order each frame
        let index = if let Some(offset) = kinds.slots[cursor..].iter().position(matches) {
            kinds.slots.swap(cursor, cursor + offset);
            kinds.cursor += 1;
            cursor
        } else if let Some(index) = kinds.slots[..cursor].iter().position(matches) {
            index
        } else {
            let value = (initial, K::Output::default());
            // reserve first so a panic cannot lose a reused cell
            kinds.slots.reserve(1);
            let free = &mut kinds.kinds[kind as usize].free;
            let offset = match *free {
                Some(offset) => {
                    let cell = kinds
                        .state
                        .as_ptr()
                        .wrapping_add(offset as usize)
                        .cast::<Cell<(K, K::Output)>>();
                    // safety: free cells of this kind are dead cells of this layout holding the next link
                    unsafe {
                        *free = (*cell).next;
                        cell.write(Cell { value });
                    }
                    offset
                }
                None => kinds.state.push(Cell { value }).byte(),
            };
            kinds.slots.push(Slot {
                id,
                node: UNRESOLVED,
                end: 0,
                offset,
                kind,
            });
            let last = kinds.slots.len() - 1;
            kinds.slots.swap(cursor, last);
            kinds.cursor += 1;
            cursor
        };
        let slot = &mut kinds.slots[index];
        slot.node = node.map_or(UNRESOLVED, |node| u32::try_from(node).expect("too many frame nodes"));
        // safety: slots of this kind point at live cells holding K and its output
        unsafe { (*kinds.state.as_ptr().add(slot.offset as usize).cast::<(K, K::Output)>()).1 }
    }

    pub fn begin(&mut self, input: &Input<T>, time: Duration) {
        self.focus_taken = false;
        self.stolen.clear();
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
                // a press away from any focusable widget clears focus
                if button == PointerButton::Primary {
                    self.focused = None;
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
                    // a tap away from any focusable widget clears focus
                    if phase == TouchPhase::Up && self.contacts[index].tap {
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
                let previous = self.wheel.filter(|wheel| wheel.active);
                let continuing = continuous && phase != ScrollPhase::Started && previous.is_some();
                // a continuous ending without a sequence has nothing to end
                let stray = continuous && matches!(phase, ScrollPhase::Ended | ScrollPhase::Cancelled) && !continuing;
                self.wheel = Some(WheelState {
                    target: if continuing {
                        previous.and_then(|wheel| wheel.target)
                    } else {
                        self.hit(position).map(|hit| hit.node)
                    },
                    owner: previous.filter(|_| continuing).and_then(|wheel| wheel.owner),
                    event: (!stray).then_some(Scroll {
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
                    active: continuous && matches!(phase, ScrollPhase::Started | ScrollPhase::Moved),
                    cancelled: previous.filter(|_| !continuing).and_then(|wheel| wheel.owner),
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
        if !matches!(input, Input::PointerLeave) {
            self.hover = self
                .position
                .and_then(|position| self.hit(position))
                .map(|hit| hit.node);
        }
        self.hover_key = self.hover_key(self.position.and(self.hover));
        // deepest kinds see contacts first
        self.deadline = None;
        let mut kinds = std::mem::take(&mut self.kinds);
        kinds.cursor = 0;
        let update = |slot: &Slot, state: &mut Self| {
            (kinds.kinds[slot.kind as usize].update)(
                &kinds.state,
                slot.offset,
                &mut InteractCx {
                    id: slot.id,
                    node: slot.node,
                    end: slot.end,
                    state,
                    input,
                    time,
                },
            );
        };
        for slot in kinds.slots.iter().rev() {
            update(slot, self);
        }
        // claimants updated before an ancestor captured their contact, so they update again
        for slot in kinds.slots.iter().rev() {
            if self.stolen.contains(&slot.id) {
                update(slot, self);
            }
        }
        self.kinds = kinds;
    }

    fn press(&mut self, source: Source, position: Point<T>, modifiers: Modifiers, time: Duration) {
        for index in 0..self.contacts.len() {
            if self.contacts[index].source == source {
                self.update(index, self.contacts[index].position, Phase::Cancelled);
            }
        }
        // ids wrap past zero, only live contacts need distinct ids
        let id = NonZeroU32::new(self.next_contact.wrapping_add(1)).unwrap_or(NonZeroU32::MIN);
        self.next_contact = id.get();
        let target = self.hit(position).map(|hit| Target {
            id: hit.id,
            node: hit.node,
        });
        self.contacts.push(TrackedContact {
            id: ContactId(id),
            source,
            route: Route::Pending { target, claim: None },
            phase: Phase::Started,
            origin: position,
            position,
            delta: Point::ZERO,
            started: time,
            modifiers,
            tap: true,
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
        tracked.tap &= x * x + y * y < TAP_SLOP * TAP_SLOP;
    }

    /// identifies the widgets under the pointer to notice when layout changes them
    fn hover_key(&self, node: Option<usize>) -> u64 {
        let Some(node) = node else {
            return 0;
        };
        self.hits
            .iter()
            .filter(|hit| hit.node <= node && node <= hit.end)
            .fold(0, |key: u64, hit| {
                let mut hasher = FxHasher::default();
                hit.id.hash(&mut hasher);
                key.wrapping_add(hasher.finish())
            })
    }

    fn hit(&self, position: Point<T>) -> Option<HitItem<T>> {
        self.hits.iter().rev().find(|hit| self.contains(hit, position)).copied()
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
    let kinds = &mut frame.interaction.kinds;
    for slot in &kinds.slots[kinds.cursor..] {
        let free = &mut kinds.kinds[slot.kind as usize].free;
        let cell = kinds.state.as_ptr().wrapping_add(slot.offset as usize);
        // safety: unrequested cells are dead and every cell starts with room for the link
        unsafe { cell.cast::<Option<u32>>().write(*free) };
        *free = Some(slot.offset);
    }
    kinds.slots.truncate(kinds.cursor);
    for slot in &mut kinds.slots {
        // named widgets find their nodes now that the build placed them
        if slot.node == UNRESOLVED
            && let Some(index) = frame.nodes.iter().position(|stored| stored.widget_id == slot.id)
        {
            slot.node = index as u32;
            frame.nodes[index].interactive = true;
        }
        slot.end = frame
            .nodes
            .get(slot.node as usize)
            .map_or(0, |stored| stored.subtree_end);
    }
    // stable insertion sort stays near linear since builds request in preorder
    let slots = &mut kinds.slots;
    for sorted in 1..slots.len() {
        let mut index = sorted;
        while index > 0 && slots[index - 1].node > slots[index].node {
            slots.swap(index - 1, index);
            index -= 1;
        }
    }
    let nodes = &frame.nodes;
    let hits = &frame.interaction.hits;
    let node = |id| {
        nodes
            .iter()
            .position(|stored| stored.interactive && stored.widget_id == id)
    };
    let next_route = |route| match route {
        Route::Pending { target, claim } => {
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
            Some(Route::Pending {
                target: Some(Target {
                    id: target,
                    node: node(target)?,
                }),
                claim: claim.filter(|claim| node(*claim).is_some()),
            })
        }
        Route::Captured { owner } => node(owner).map(|_| route),
        Route::Retired => None,
    };
    for tracked in &mut frame.interaction.contacts {
        let next = next_route(tracked.route).unwrap_or(Route::Retired);
        // a live contact losing its holder changes that holder's output
        frame.frame_requested |=
            !matches!(tracked.phase, Phase::Ended | Phase::Cancelled) && next.holder() != tracked.route.holder();
        tracked.route = next;
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
    frame.areas_previous.clear();
    frame.interaction.hits.clear();
    frame.areas_requested.sort_unstable();
    frame.areas_requested.dedup();
    if !frame.areas_requested.is_empty() || !frame.interaction.kinds.slots.is_empty() {
        let count = frame.nodes.len();
        let placements = &frame.placements[..count];
        let mut collect = {
            #[inline(always)]
            |index: usize, stored: &super::StoredNode, area: Rect<C::Scalar>| {
                let id = stored.widget_id;
                if !frame.areas_requested.is_empty() && frame.areas_requested.binary_search(&id).is_ok() {
                    frame.areas_previous.push((id, area));
                }
                if !stored.interactive {
                    return;
                }
                let hit = stored
                    .bounds
                    .index()
                    .map_or(Sides::all(C::Scalar::ZERO), |index| frame.bounds[index].hit);
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
                    node: index,
                    end: stored.subtree_end as usize,
                });
            }
        };
        if frame.paint_order.is_empty() {
            for (index, (stored, placement)) in nodes.iter().zip(placements).enumerate() {
                collect(index, stored, placement.area);
            }
        } else {
            for id in &frame.paint_order[..count] {
                collect(id.index(), &nodes[id.index()], placements[id.index()].area);
            }
        }
    }
    frame.areas_previous.sort_unstable_by_key(|(id, _)| *id);
    let interaction = &mut frame.interaction;
    let hover = interaction
        .position
        .and_then(|position| interaction.hit(position))
        .map(|hit| hit.node);
    frame.frame_requested |= interaction.hover_key(hover) != interaction.hover_key;
}

#[derive(Clone, Copy, PartialEq)]
enum Route {
    Pending { target: Option<Target>, claim: Option<Id> },
    Captured { owner: Id },
    Retired,
}

impl Route {
    fn holder(self) -> Option<Id> {
        match self {
            Route::Pending { claim, .. } => claim,
            Route::Captured { owner } => Some(owner),
            Route::Retired => None,
        }
    }
}

/// a pending contact's deepest widget and its node in the previous layout
#[derive(Clone, Copy, PartialEq)]
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
    tap: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Mouse(PointerButton),
    Touch(u64),
}

#[derive(Clone, Copy)]
struct WheelState {
    // node under the pointer when the sequence started
    target: Option<usize>,
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
    node: usize,
    end: usize,
}

/// retained interaction kinds and their state
#[derive(Default)]
struct Kinds<T> {
    // preorder by node so reverse iteration visits descendants first
    slots: Vec<Slot>,
    // slots requested by this build are moved before the cursor
    cursor: usize,
    kinds: Vec<Kind<T>>,
    // cells of every kind, reused through each kind's free list
    state: Bump,
}

#[derive(Clone, Copy)]
struct Slot {
    id: Id,
    // node and subtree end in the previous layout
    node: u32,
    end: u32,
    offset: u32,
    kind: u16,
}

struct Kind<T> {
    type_id: TypeId,
    update: fn(&Bump, u32, &mut InteractCx<'_, T>),
    // dead cells of this kind linked through their first bytes
    free: Option<u32>,
}

/// a kind and its latest output, which holds the free link once it is dead
#[derive(Clone, Copy)]
#[repr(C)]
union Cell<S: Copy> {
    value: S,
    next: Option<u32>,
}
