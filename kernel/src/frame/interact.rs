use super::*;
use crate::internal::bump::Offset;

// slot node for a named widget missing from the layout
const UNRESOLVED: u32 = u32::MAX;

/// the previous layout and input routed to one interaction kind
pub struct InteractCx<'a, T> {
    id: Id,
    node: u32,
    end: u32,
    contacts: &'a mut [TrackedContact<T>],
    scroll: &'a mut Option<TrackedScroll>,
    focused: &'a mut Option<Id>,
    focus_taken: &'a mut bool,
    hover: Option<u32>,
    deadline: &'a mut Option<Duration>,
    input: &'a Input<T>,
    time: Duration,
}

impl<T: Scalar> InteractCx<'_, T> {
    pub fn time(&self) -> Duration {
        self.time
    }

    /// builds a frame at this time without input
    pub fn wake_at(&mut self, time: Duration) {
        *self.deadline = Some(self.deadline.map_or(time, |deadline| deadline.min(time)));
    }

    /// every live press and touch
    pub fn contacts(&self) -> Contacts<'_, T> {
        Contacts {
            contacts: self.contacts.iter(),
            id: self.id,
            node: self.node,
            end: self.end,
        }
    }

    /// keeps a pending contact from ancestors until the next update
    ///
    /// fails for a contact this widget already captured. widgets see contacts claimed by others
    /// only once those claims lapse
    pub fn claim(&mut self, contact: ContactId) -> bool {
        let id = self.id;
        if let Some(Route::Pending { claim, .. }) = self.pending(contact)
            && claim.is_none_or(|claim| claim == id)
        {
            *claim = Some(id);
            return true;
        }
        false
    }

    /// takes a pending contact within this widget from every other widget until it ends
    ///
    /// a contact another widget claimed during this update stays with it
    pub fn capture(&mut self, contact: ContactId) -> bool {
        let owner = self.id;
        let (node, end) = (self.node, self.end);
        if let Some(tracked) = self.contacts.iter_mut().find(|tracked| tracked.id == contact)
            && let Route::Pending {
                target: Some(target),
                claim,
            } = tracked.route
            && node <= target.node
            && target.node <= end
            && claim.is_none_or(|claim| claim == owner)
        {
            tracked.route = Route::Captured { owner };
            return true;
        }
        false
    }

    pub fn hovered(&self) -> bool {
        self.hover.is_some_and(|node| self.contains(node))
    }

    pub fn focused(&self) -> bool {
        *self.focused == Some(self.id)
    }

    /// focuses this widget unless a deeper widget took focus during this update
    pub fn focus(&mut self) {
        if !*self.focus_taken {
            *self.focused = Some(self.id);
            *self.focus_taken = true;
        }
    }

    /// this frame's input or none during the first update
    pub fn input(&self) -> Input<T> {
        *self.input
    }

    /// scrolling over this widget, or the cancellation of a sequence it held
    pub fn scroll(&self) -> Option<ScrollEvent> {
        let scroll = (*self.scroll)?;
        let event = scroll.event.filter(|_| match scroll.owner {
            Some(owner) => owner == self.id,
            None => scroll.target.is_some_and(|node| self.contains(node)),
        });
        event.or((scroll.cancelled == Some(self.id)).then_some(ScrollEvent {
            phase: Phase::Cancelled,
            delta: Point::ZERO,
            continuous: true,
        }))
    }

    /// keeps a continuous scroll sequence over this widget until it ends
    pub fn capture_scroll(&mut self) {
        let id = self.id;
        let (node, end) = (self.node, self.end);
        if let Some(scroll) = self.scroll
            && scroll.owner.is_none()
            && scroll.target.is_some_and(|target| node <= target && target <= end)
        {
            scroll.owner = Some(id);
        }
    }

    fn contains(&self, node: u32) -> bool {
        self.node <= node && node <= self.end
    }

    fn pending(&mut self, contact: ContactId) -> Option<&mut Route> {
        let (node, end) = (self.node, self.end);
        let tracked = self.contacts.iter_mut().find(|tracked| tracked.id == contact)?;
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

impl<T: Scalar> Iterator for Contacts<'_, T> {
    type Item = Contact<T>;

    fn next(&mut self) -> Option<Contact<T>> {
        let tracked = self.contacts.next()?;
        let within = |node| self.node <= node && node <= self.end;
        let free = match tracked.route {
            Route::Pending {
                target: Some(target),
                claim,
            } => within(target.node) && claim.is_none_or(|claim| claim == self.id),
            Route::Captured { owner } => owner == self.id,
            Route::Pending { target: None, .. } | Route::Retired => false,
        };
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
            over: tracked.hit.is_some_and(within),
            free,
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
    scroll: Option<TrackedScroll>,
    hits: Vec<HitItem<T>>,
    kinds: Kinds<T>,
}

impl<T: Scalar> InteractionState<T> {
    pub fn interact<K: Interact<T>>(&mut self, id: Id, mut initial: K, time: Duration) -> K::Response {
        let kinds = &mut self.kinds;
        let type_id = TypeId::of::<K>();
        let kind = kinds
            .kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                assert!(kinds.kinds.len() <= u16::MAX as usize, "too many interaction kinds");
                kinds.kinds.push(Kind {
                    type_id,
                    update: |kinds, index, cx| {
                        let (kind, response) = kinds.get_mut::<K>(index);
                        *response = kind.update(cx);
                    },
                    free: None,
                });
                kinds.kinds.len() - 1
            });
        let kind = kind as u16;
        let index = match kinds.find(id, Some(kind)) {
            Err(bucket) => {
                let mut focused = self.focused;
                let response = initial.update(&mut InteractCx {
                    id,
                    node: UNRESOLVED,
                    end: 0,
                    contacts: &mut [],
                    scroll: &mut None,
                    focused: &mut focused,
                    focus_taken: &mut true,
                    hover: None,
                    deadline: &mut self.deadline,
                    input: &Input::None,
                    time,
                });
                let value = (initial, response);
                // reserve first so a panic cannot lose a reused cell
                kinds.slots.reserve(1);
                let free = &mut kinds.kinds[kind as usize].free;
                let offset = match *free {
                    Some(offset) => {
                        let offset = offset.get() - 1;
                        // safety: free cells of this kind are dead cells of this layout holding the next link
                        unsafe {
                            let cell = kinds.state.get_mut::<Cell<(K, K::Response)>>(Offset::new(offset));
                            *free = cell.next;
                            *cell = Cell { value };
                        }
                        offset
                    }
                    None => kinds.state.push(Cell { value }).byte(),
                };
                kinds.insert(
                    bucket,
                    Slot {
                        id,
                        node: UNRESOLVED,
                        offset,
                        order: UNRESOLVED,
                        kind,
                    },
                )
            }
            Ok(index) => index,
        };
        let slot = &mut kinds.slots[index];
        if slot.order == UNRESOLVED {
            slot.order = kinds.requested;
            kinds.requested += 1;
        }
        kinds.get_mut::<K>(index).1
    }

    pub fn begin(&mut self, input: &Input<T>, time: Duration, nodes: &[StoredNode]) {
        self.contacts
            .retain(|tracked| !matches!(tracked.phase, Phase::Ended | Phase::Cancelled));
        for tracked in &mut self.contacts {
            tracked.phase = Phase::Held;
            tracked.delta = Point::ZERO;
            // claims last one update, claimants renew them
            if let Route::Pending { claim, .. } = &mut tracked.route {
                *claim = None;
            }
        }
        if let Some(scroll) = &mut self.scroll {
            if scroll.active {
                scroll.event = None;
                scroll.cancelled = None;
            } else {
                self.scroll = None;
            }
        }
        match *input {
            Input::PointerDown {
                position, modifiers, ..
            }
            | Input::PointerMove { position, modifiers }
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
                if let Input::PointerDown { button, .. } = *input {
                    // a press away from any focusable widget clears focus
                    if button == PointerButton::Primary {
                        self.focused = None;
                    }
                    self.press(Source::Mouse(button), position, modifiers, time);
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
                    // a release nothing captured clears focus unless a focusable widget takes it
                    if phase == TouchPhase::Up && matches!(self.contacts[index].route, Route::Pending { .. }) {
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
                let previous = self.scroll.filter(|scroll| scroll.active);
                let continuing = continuous && phase != ScrollPhase::Started && previous.is_some();
                // a continuous ending without a sequence has nothing to end
                let stray = continuous && matches!(phase, ScrollPhase::Ended | ScrollPhase::Cancelled) && !continuing;
                self.scroll = Some(TrackedScroll {
                    target: if continuing {
                        previous.and_then(|scroll| scroll.target)
                    } else {
                        hit(&self.hits, &self.clips, position).map(|hit| hit.node)
                    },
                    owner: previous.filter(|_| continuing).and_then(|scroll| scroll.owner),
                    event: (!stray).then_some(ScrollEvent {
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
                    cancelled: previous.filter(|_| !continuing).and_then(|scroll| scroll.owner),
                });
            }
            Input::Cancel => {
                self.position = None;
                for index in 0..self.contacts.len() {
                    self.update(index, self.contacts[index].position, Phase::Cancelled);
                }
                if let Some(scroll) = &mut self.scroll {
                    scroll.active = false;
                    scroll.event = Some(ScrollEvent {
                        phase: Phase::Cancelled,
                        delta: Point::ZERO,
                        continuous: true,
                    });
                }
            }
            Input::None | Input::Text(_) | Input::Key(_) => {}
        }
        let hover = self
            .position
            .and_then(|position| hit(&self.hits, &self.clips, position))
            .map(|hit| hit.node);
        for tracked in &mut self.contacts {
            tracked.hit = hit(&self.hits, &self.clips, tracked.position).map(|hit| hit.node);
        }
        // deepest nodes update first
        self.deadline = None;
        let mut focus_taken = false;
        let kinds = &mut self.kinds;
        for index in (0..kinds.slots.len()).rev() {
            let slot = kinds.slots[index];
            (kinds.kinds[slot.kind as usize].update)(
                kinds,
                index,
                &mut InteractCx {
                    id: slot.id,
                    node: slot.node,
                    end: nodes.get(slot.node as usize).map_or(0, |stored| stored.subtree_end),
                    contacts: &mut self.contacts,
                    scroll: &mut self.scroll,
                    focused: &mut self.focused,
                    focus_taken: &mut focus_taken,
                    hover,
                    deadline: &mut self.deadline,
                    input,
                    time,
                },
            );
        }
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
        let target = hit(&self.hits, &self.clips, position).map(|hit| Target {
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
            hit: None,
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
    }
}

pub fn resolve<C: Context>(frame: &mut Frame<C>) {
    let retained = &mut frame.interaction.kinds;
    for slot in &mut retained.slots {
        slot.node = UNRESOLVED;
    }
    if !retained.slots.is_empty() {
        for (index, stored) in frame.nodes.iter().enumerate() {
            retained.bind(stored.widget_id, index as u32);
        }
    }
    let Kinds {
        slots, kinds, state, ..
    } = retained;
    slots.retain(|slot| {
        if slot.order != UNRESOLVED {
            return true;
        }
        let free = &mut kinds[slot.kind as usize].free;
        let cell = state.as_ptr().wrapping_add(slot.offset as usize);
        // safety: unrequested cells are dead and every cell starts with room for the link
        unsafe { cell.cast::<Option<NonZeroU32>>().write(*free) };
        *free = NonZeroU32::new(slot.offset + 1);
        false
    });
    // adding one puts missing widgets first so reverse updates reach them last
    slots.sort_unstable_by_key(|slot| (slot.node.wrapping_add(1), slot.order));
    retained.rebuild();
    retained.requested = 0;
    for slot in &mut retained.slots {
        slot.order = UNRESOLVED;
        if let Some(stored) = frame.nodes.get_mut(slot.node as usize) {
            stored.interactive = true;
        }
    }
    let retained = &*retained;
    let nodes = &frame.nodes;
    let hits = &frame.interaction.hits;
    let node = |id| {
        let slot = &retained.slots[retained.find(id, None).ok()?];
        (slot.node != UNRESOLVED).then_some(slot.node as usize)
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
            let (target, index) = if let Some(index) = node(target) {
                (target, index)
            } else {
                hits.iter()
                    .filter(|hit| hit.node <= previous && previous <= hit.end)
                    .filter_map(|hit| node(hit.id).map(|index| (hit, index)))
                    .max_by_key(|(hit, _)| hit.node)
                    .map(|(hit, index)| (hit.id, index))?
            };
            Some(Route::Pending {
                target: Some(Target {
                    id: target,
                    node: index as u32,
                }),
                claim,
            })
        }
        Route::Captured { owner } => node(owner).map(|_| route),
        Route::Retired => None,
    };
    for tracked in &mut frame.interaction.contacts {
        tracked.route = next_route(tracked.route).unwrap_or(Route::Retired);
    }
    if frame.interaction.focused.is_some_and(|id| node(id).is_none()) {
        frame.interaction.focused = None;
    }
    if let Some(scroll) = &mut frame.interaction.scroll
        && scroll.owner.is_some_and(|id| node(id).is_none())
    {
        // the rest of a removed owner's sequence reaches no one, rather than whatever moved into its node
        scroll.owner = None;
        scroll.event = None;
        scroll.target = None;
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
                if frame.areas_requested.binary_search(&id).is_ok() {
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
                    node: index as u32,
                    end: stored.subtree_end,
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
}

fn hit<T: Scalar>(hits: &[HitItem<T>], clips: &[ResolvedClip<T>], position: Point<T>) -> Option<HitItem<T>> {
    hits.iter()
        .rev()
        .find(|hit| {
            if !hit.area.contains(position) {
                return false;
            }
            let mut clip = hit.clip;
            while let Some(index) = clip.index() {
                let stored = &clips[index];
                if !stored.shape.contains(position) {
                    return false;
                }
                clip = stored.parent;
            }
            true
        })
        .copied()
}

#[derive(Clone, Copy, PartialEq)]
enum Route {
    Pending { target: Option<Target>, claim: Option<Id> },
    Captured { owner: Id },
    Retired,
}

/// a pending contact's deepest widget and its node in the previous layout
#[derive(Clone, Copy, PartialEq)]
struct Target {
    id: Id,
    node: u32,
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
    // node under the position in the previous layout
    hit: Option<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Mouse(PointerButton),
    Touch(u64),
}

#[derive(Clone, Copy)]
struct TrackedScroll {
    // node under the pointer when the sequence started
    target: Option<u32>,
    owner: Option<Id>,
    event: Option<ScrollEvent>,
    active: bool,
    cancelled: Option<Id>,
}

#[derive(Clone, Copy)]
struct HitItem<T> {
    id: Id,
    area: Rect<T>,
    clip: ResolvedClipId,
    node: u32,
    end: u32,
}

/// retained interaction kinds and their state
#[derive(Default)]
struct Kinds<T> {
    // sorted by node and request order, then slots added by the current build
    slots: Vec<Slot>,
    // slot indices, at most half full
    table: Vec<u32>,
    requested: u32,
    kinds: Vec<Kind<T>>,
    // cells of every kind, reused through each kind's free list
    state: Bump,
}

impl<T> Kinds<T> {
    fn get_mut<K: Interact<T>>(&mut self, index: usize) -> &mut (K, K::Response) {
        let slot = &self.slots[index];
        assert_eq!(
            self.kinds[slot.kind as usize].type_id,
            TypeId::of::<K>(),
            "interaction kind mismatch"
        );
        // safety: the slot is live and its kind was checked above
        unsafe { self.state.get_mut(Offset::new(slot.offset)) }
    }

    // slot index on a hit, insertion bucket on a miss
    fn find(&self, id: Id, kind: Option<u16>) -> Result<usize, usize> {
        if self.table.is_empty() {
            return Err(0);
        }
        let bucket = self.probe(id, kind);
        match self.table[bucket] {
            UNRESOLVED => Err(bucket),
            index => Ok(index as usize),
        }
    }

    fn insert(&mut self, bucket: usize, slot: Slot) -> usize {
        let index = self.slots.len();
        assert!(index < UNRESOLVED as usize, "too many interaction slots");
        self.slots.push(slot);
        if self.slots.len() > self.table.len() / 2 {
            self.rebuild();
        } else {
            self.table[bucket] = index as u32;
        }
        index
    }

    fn bind(&mut self, id: Id, node: u32) {
        let mut bucket = self.probe(id, None);
        while self.table[bucket] != UNRESOLVED {
            let slot = &mut self.slots[self.table[bucket] as usize];
            if slot.id == id {
                slot.node = node;
            }
            bucket = (bucket + 1) & (self.table.len() - 1);
        }
    }

    fn rebuild(&mut self) {
        let len = self.slots.len();
        if len == 0 {
            self.table.clear();
            return;
        }
        // grow above half full and shrink below one eighth
        // to avoid resizing on small count changes
        if len < self.table.len() / 8 {
            self.table.truncate((len * 2).next_power_of_two());
        }
        self.table.fill(UNRESOLVED);
        if len > self.table.len() / 2 {
            self.table.resize((len * 2).next_power_of_two(), UNRESOLVED);
        }
        for (index, slot) in self.slots.iter().enumerate() {
            let bucket = self.probe(slot.id, Some(slot.kind));
            self.table[bucket] = index as u32;
        }
    }

    fn probe(&self, id: Id, kind: Option<u16>) -> usize {
        let mask = self.table.len() - 1;
        let mut bucket = (id.0.get() as usize) >> (usize::BITS - self.table.len().trailing_zeros());
        loop {
            let index = self.table[bucket];
            if index == UNRESOLVED {
                return bucket;
            }
            let slot = &self.slots[index as usize];
            if slot.id == id && kind.is_none_or(|kind| slot.kind == kind) {
                return bucket;
            }
            bucket = (bucket + 1) & mask;
        }
    }
}

#[derive(Clone, Copy)]
struct Slot {
    id: Id,
    // in the latest build, or unresolved for a named widget missing from it
    node: u32,
    offset: u32,
    // first request in the current build, or unresolved when unrequested
    order: u32,
    kind: u16,
}

struct Kind<T> {
    type_id: TypeId,
    update: fn(&mut Kinds<T>, usize, &mut InteractCx<'_, T>),
    // dead cells of this kind linked by byte offset plus one
    free: Option<NonZeroU32>,
}

/// a kind and its latest response, which holds the free link once it is dead
#[derive(Clone, Copy)]
#[repr(C)]
union Cell<S: Copy> {
    value: S,
    next: Option<NonZeroU32>,
}
