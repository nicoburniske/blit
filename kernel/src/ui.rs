use crate::*;

/// typestate modes for [`Ui`]
///
/// every mode can query its target and access shared frame services
pub mod state {
    use super::PhantomData;

    /// a node without a layout that may establish one
    pub struct Build;

    /// a child without a layout, with layout item `I`
    pub struct Child<I>(PhantomData<I>);

    /// a laid-out node that may create children
    pub struct Open<L>(PhantomData<L>);

    /// adds content and interactions to an existing node through [`crate::Ui::insert`]
    pub struct Insert;

    /// another node referenced by id through [`crate::Ui::node`], which may be built later
    pub struct Ref;

    /// modes that own their node and may set its properties
    pub trait Owned {}

    impl Owned for Build {}
    impl<I> Owned for Child<I> {}
    impl<L> Owned for Open<L> {}

    /// modes that act on their own node rather than a target
    pub trait Local {}

    impl Local for Build {}
    impl<I> Local for Child<I> {}
    impl<L> Local for Open<L> {}
    impl Local for Insert {}

    /// modes whose node has no layout yet
    pub trait NoLayout: Owned + Local {}

    impl NoLayout for Build {}
    impl<I> NoLayout for Child<I> {}
}

/// scoped handle for building a frame node
///
/// its [`state`] mode determines which operations are available
pub struct Ui<'ui, C: Context, S = state::Build> {
    inner: UiInner<'ui, C>,
    marker: PhantomData<S>,
}

impl<'ui, C: Context, S> Ui<'ui, C, S> {
    /// the target's stable id
    pub fn id(&self) -> Id {
        self.inner.target
    }

    /// the target's area from the previous layout
    pub fn area(&self) -> Option<Rect<C::Scalar>> {
        let kernel = &*self.inner.kernel;
        let old = kernel.slots.old(self.inner.target)?;
        kernel.old_placements.get(old).map(|placement| placement.area)
    }

    /// adds `interaction` to the target and returns its latest response
    ///
    /// one interaction of each kind per target, repeated requests share it
    pub fn interact<K: Interact<C>>(&mut self, interaction: K) -> K::Response {
        let kernel = &mut *self.inner.kernel;
        kernel
            .interaction
            .interact::<C, K>(self.inner.target, interaction, kernel.time)
    }

    /// animates toward `target`, keyed by the target and call site, then by call order within a frame
    #[track_caller]
    pub fn animate(&mut self, target: f32, animation: impl Into<Animation>, mut options: AnimationOptions) -> f32 {
        let call = Call::new(self.inner.target, Location::caller());
        let kernel = &mut *self.inner.kernel;
        // cleared so stored options stay equal across frames and do not retarget
        let initial = options.initial.take().unwrap_or(target);
        let state = AnimationState::update(&mut kernel.animations, call, initial);
        match &mut state.playback {
            Playback::Target(animator) => {
                animator
                    .advance([target], [true], animation.into(), options, kernel.time, true)
                    .0[0]
            }
            Playback::Loop { value, .. } => {
                let mut animator = Animator::new([*value], Animation::default());
                let value = animator
                    .advance([target], [true], animation.into(), options, kernel.time, true)
                    .0[0];
                state.playback = Playback::Target(animator);
                value
            }
        }
    }

    #[track_caller]
    pub fn animate_loop(&mut self, mut tween: Tween) -> f32 {
        tween.duration = tween.duration.max(0.0);
        if tween.duration == 0.0 {
            return self.animate(0.0, Animation::default(), AnimationOptions::default());
        }
        let call = Call::new(self.inner.target, Location::caller());
        let kernel = &mut *self.inner.kernel;
        let state = AnimationState::update(&mut kernel.animations, call, 0.0);
        let now = kernel.time.as_secs_f64();
        let time = match state.playback {
            Playback::Loop {
                tween: previous, time, ..
            } if previous == tween => time,
            _ => now,
        };
        let progress = (now - time).max(0.0) as f32 / tween.duration;
        let value = tween.easing.sample(progress % 1.0).0;
        state.playback = Playback::Loop { tween, time, value };
        value
    }

    /// fires once, `duration` after the target first calls it from this site or after a reset
    #[track_caller]
    pub fn timer(&mut self, duration: Duration) -> Timer<'_> {
        let call = Call::new(self.inner.target, Location::caller());
        let kernel = &mut *self.inner.kernel;
        TimerState::update(&mut kernel.timers, call, duration, false, kernel.time)
    }

    /// fires every `period`, counting from when the target first calls it from this site or from a reset
    #[track_caller]
    pub fn interval(&mut self, period: Duration) -> Timer<'_> {
        assert!(!period.is_zero(), "an interval must be nonzero");
        let call = Call::new(self.inner.target, Location::caller());
        let kernel = &mut *self.inner.kernel;
        TimerState::update(&mut kernel.timers, call, period, true, kernel.time)
    }

    /// focuses the target from the next build while it requests an interaction
    pub fn focus(&mut self) {
        self.inner.kernel.focus = FocusRequest::Focus(self.inner.target);
    }

    /// whether the target holds focus as this build began
    pub fn focused(&self) -> bool {
        self.inner.kernel.interaction.focused == Some(self.inner.target)
    }

    /// whether focus changed from the previous build
    pub fn focus_moved(&self) -> bool {
        let interaction = &self.inner.kernel.interaction;
        interaction.focused != interaction.focus_seen
    }

    /// clears focus from the next build
    pub fn clear_focus(&mut self) {
        self.inner.kernel.focus = FocusRequest::Clear;
    }

    /// access context during frame construction
    pub fn context(&self) -> &C {
        self.inner.context
    }

    /// access mutable context during frame construction
    pub fn context_mut(&mut self) -> &mut C {
        self.inner.context
    }

    /// the mouse cursor position, independent of touches
    pub fn mouse_position(&self) -> Option<Point<C::Scalar>> {
        self.inner.kernel.interaction.mouse_position
    }

    /// the position of a contact in this frame
    ///
    /// ended and cancelled contacts remain available until the next build
    pub fn contact_position(&self, id: ContactId) -> Option<Point<C::Scalar>> {
        self.inner.kernel.interaction.contact_position(id)
    }

    /// this frame's input regardless of focus or claims
    ///
    /// handle keyboard shortcuts through an interaction to respect input routing.
    pub fn input(&self) -> Input<C::Scalar> {
        self.inner.kernel.input
    }

    pub fn screen(&self) -> Rect<C::Scalar> {
        self.inner.kernel.screen
    }

    pub fn time(&self) -> Duration {
        self.inner.kernel.time
    }

    pub fn request_frame(&mut self) {
        self.inner.kernel.request_frame();
    }
}

impl<'ui, C: Context, S: state::Local> Ui<'ui, C, S> {
    /// refers to the node with `id`, which may be built later, and keeps its id through this build
    #[inline]
    pub fn node(&mut self, id: Id) -> Ui<'_, C, state::Ref> {
        self.inner.kernel.slots.keep(id);
        Ui {
            inner: self.inner.borrow(id),
            marker: PhantomData,
        }
    }

    /// resolves `key` in `scope` to its id, taking a slot if nothing holds it yet
    #[inline]
    pub fn resolve(&mut self, scope: Scope, key: impl Key) -> Id {
        let id = self.inner.kernel.slots.name(scope.0, key.value());
        self.inner.kernel.slots.keep(id);
        id
    }

    /// a scope for keys of this node and call site, so keys of unrelated code never meet
    ///
    /// scopes made at one call site repeat within a build, told apart by their order
    #[track_caller]
    pub fn scope(&mut self) -> Scope {
        let kernel = &mut *self.inner.kernel;
        let mut call = Call::new(self.inner.target, Location::caller());
        let index = loop {
            match kernel.scopes.binary_search_by_key(&call, |scope| scope.call) {
                Ok(index) if kernel.slots.is_kept(kernel.scopes[index].id) => call.repeat += 1,
                Ok(index) => break index,
                Err(index) => {
                    let id = kernel.slots.alloc();
                    kernel.scopes.insert(index, CallScope { call, id });
                    break index;
                }
            }
        };
        let id = kernel.scopes[index].id;
        kernel.slots.keep(id);
        Scope(id)
    }

    /// inserts content into the current node
    #[inline]
    pub fn insert<W: Widget<C, state::Insert>>(&mut self, widget: W) -> W::Response {
        widget.build(Ui {
            inner: self.inner.borrow(self.inner.target),
            marker: PhantomData,
        })
    }

    /// schedules a deferred paint closure on the current node
    #[inline]
    pub fn paint<F>(&mut self, paint: F)
    where
        F: Fn(&mut PaintCx<'_, C>, Rect<C::Scalar>) + 'static,
    {
        paint::insert_paint(self.inner.kernel, self.inner.node, paint);
    }

    /// registers a deferred measurement closure on the current node
    ///
    /// measurement may be skipped under tight constraints. painting must not
    /// depend on prior measurement.
    #[inline]
    pub fn measure<F>(&mut self, measure: F)
    where
        F: Fn(&mut C, Constraints<C::Scalar>) -> Size<C::Scalar> + 'static,
    {
        paint::insert_measure(self.inner.kernel, self.inner.node, measure);
    }

    /// the focused node, if it was beneath this one in the previous layout
    pub fn focus_within(&self) -> Option<Id> {
        let kernel = &*self.inner.kernel;
        let focused = kernel.interaction.focused?;
        let node = kernel.slots.old(self.inner.target)?;
        let inside = kernel.slots.old(focused)?;
        (node < inside && inside <= kernel.old_nodes[node].subtree_end as usize).then_some(focused)
    }

    /// the id of this node's structural parent, or its own for the root
    pub fn parent_id(&self) -> Id {
        let nodes = &self.inner.kernel.nodes;
        nodes[nodes[self.inner.node.index()].parent.index()].id
    }

    /// temporary storage that stays usable while building this node
    pub fn scratch(&self) -> &'ui ScratchBuffer {
        self.inner.scratch
    }

    pub fn store(&self) -> &FrameStore
    where
        C: HasFrameStore,
    {
        self.inner.context.store()
    }

    pub fn store_mut(&mut self) -> crate::FrameStoreMut<'_>
    where
        C: HasFrameStore,
    {
        self.inner.context.store_mut()
    }
}

impl<'ui, C: Context, S: state::Owned> Ui<'ui, C, S> {
    /// sets this node's size limits
    pub fn constrain(self, limits: Constraints<C::Scalar>) -> Self {
        assert!(
            limits.min.width >= C::Scalar::ZERO
                && limits.min.height >= C::Scalar::ZERO
                && limits.min.width.is_finite()
                && limits.min.height.is_finite()
                && limits.max.width >= limits.min.width
                && limits.max.height >= limits.min.height,
            "invalid local bounds"
        );
        self.inner.kernel.aux_mut(self.inner.node).limits = limits;
        self
    }

    /// places this node against the node with `target` outside its parent's flow
    ///
    /// a target not built before this node places it against its parent
    pub fn place<P: Place<C>>(self, target: Id, place: P) -> Self {
        let kernel = &mut *self.inner.kernel;
        let reference = resolve_target(kernel, self.inner.node, target);
        let id = PlaceId::new(kernel.places.len());
        kernel.places.push(StoredPlace {
            bump_id: kernel.bump.store(place),
            run: layout::place::<C, P>,
        });
        let node = &mut kernel.nodes[self.inner.node.index()];
        node.target = reference.unwrap_or(node.target);
        node.place = id;
        self
    }

    #[inline]
    pub fn motion(self, motion: Motion) -> Self {
        // kept out of line so motion inlines without moving the ui through a call
        fn store<C: Context>(kernel: &mut Kernel<C>, node: NodeId, motion: Motion) {
            match kernel.aux_mut(node).motion.index() {
                Some(index) => kernel.node_motions[index].motion = motion,
                None => {
                    kernel.aux_mut(node).motion = NodeMotionId::new(kernel.node_motions.len());
                    kernel.node_motions.push(NodeMotion { node, motion });
                }
            }
        }
        store(self.inner.kernel, self.inner.node, motion);
        self
    }

    /// selects the parent for stacking, clipping and containing size
    ///
    /// positioning stays with its anchor. a target not built before this node leaves its own parent
    #[inline]
    pub fn parent(self, target: Id) -> Self {
        if let Some(parent) = resolve_target(self.inner.kernel, self.inner.node, target) {
            self.inner.kernel.aux_mut(self.inner.node).visual_parent = parent;
        }
        self
    }

    /// clips drawing and hit testing of this node and its visual descendants to its rounded area
    pub fn clip(self, radius: impl Into<Corners<C::Scalar>>) -> Self {
        self.inner.kernel.aux_mut(self.inner.node).clip = Some(radius.into());
        self
    }

    /// sets this node's paint order among its visual siblings
    pub fn z_index(self, z_index: i16) -> Self {
        let kernel = &mut *self.inner.kernel;
        kernel.placements.resize(kernel.nodes.len(), Placement::default());
        kernel.placements[self.inner.node.index()].z_index = z_index;
        self
    }

    pub fn hit(self, hit: Sides<C::Scalar>) -> Self {
        self.inner.kernel.aux_mut(self.inner.node).hit = hit;
        self
    }
}

impl<'ui, C: Context, S: state::NoLayout> Ui<'ui, C, S> {
    /// builds a widget in this node
    pub fn build<W: Widget<C>>(self, widget: W) -> W::Response {
        widget.build(Ui {
            inner: self.inner,
            marker: PhantomData,
        })
    }

    /// establishes the current node's layout
    pub fn layout<L: Layout<C>>(self, layout: L) -> Ui<'ui, C, state::Open<L>> {
        let Ui { inner, .. } = self;
        let kernel = &mut *inner.kernel;
        let type_id = TypeId::of::<L>();
        let kind = kernel
            .layout_kinds
            .iter()
            .position(|kind| kind.type_id == type_id)
            .unwrap_or_else(|| {
                kernel.layout_kinds.push(LayoutKind {
                    type_id,
                    run: layout::run::<C, L>,
                    default_item: BumpId::NONE,
                });
                kernel.layout_kinds.len() - 1
            });
        if kernel.layout_kinds[kind].default_item.offset().is_none() {
            kernel.layout_kinds[kind].default_item = kernel.bump.store(L::Item::default());
        }
        let kind = u16::try_from(kind).expect("too many layout kinds");
        let bump_id = kernel.bump.store(layout);
        let node = &mut kernel.nodes[inner.node.index()];
        node.layout = bump_id;
        node.layout_kind = kind;
        Ui {
            inner,
            marker: PhantomData,
        }
    }
}

impl<'ui, C: Context, I: 'static> Ui<'ui, C, state::Child<I>> {
    /// sets this child's layout item
    #[inline]
    pub fn item(self, item: I) -> Ui<'ui, C, state::Build> {
        let Ui { inner, .. } = self;
        inner.kernel.nodes[inner.node.index()].item = inner.kernel.bump.store(item);
        Ui {
            inner,
            marker: PhantomData,
        }
    }
}

impl<'ui, C: Context, L: Layout<C>> Ui<'ui, C, state::Open<L>> {
    /// creates a child identified by where it is built
    ///
    /// children built from one place are told apart by the order they are built in
    #[inline]
    #[track_caller]
    pub fn child(&mut self) -> Ui<'_, C, state::Child<L::Item>> {
        let (node, old) = self.inner.child(Location::caller());
        Ui::new(self.inner.kernel, self.inner.context, self.inner.scratch, node, old)
    }

    /// creates the child keyed `key` in `scope` rather than one identified by where it is built
    ///
    /// it keeps its state as its siblings change or it moves to another parent, as its key stays.
    /// [`Ui::resolve`] with the same scope and key returns its id before it is built
    #[inline]
    #[track_caller]
    pub fn child_in(&mut self, scope: Scope, key: impl Key) -> Ui<'_, C, state::Child<L::Item>> {
        let (node, old) = self.inner.child_in(scope.0, key.value(), Location::caller());
        Ui::new(self.inner.kernel, self.inner.context, self.inner.scratch, node, old)
    }

    /// lays out a built child now and returns its current animated size under `bounds`
    ///
    /// the layout pass lays it out again with whatever bounds this node's layout gives it
    pub fn layout_child(&mut self, child: Id, bounds: Constraints<C::Scalar>) -> Size<C::Scalar> {
        let kernel = &mut *self.inner.kernel;
        let child = kernel.slots.node(child).expect("a laid out child must be built first");
        assert!(
            kernel.nodes[child.index()].place.index().is_none(),
            "measured widgets must stay in flow"
        );
        let mut bump = core::mem::take(&mut kernel.bump);
        bump.storage.prepare();
        let size = layout::resolve(kernel, &bump, self.inner.context, child, bounds, false);
        kernel.bump = bump;
        size
    }

    /// insets this node's children while painted content keeps the full box
    pub fn pad(self, padding: impl Into<Sides<C::Scalar>>) -> Self {
        let padding = padding.into();
        assert!(
            [padding.top, padding.right, padding.bottom, padding.left]
                .iter()
                .all(|side| *side >= C::Scalar::ZERO && side.is_finite()),
            "invalid padding"
        );
        self.inner.kernel.aux_mut(self.inner.node).padding = padding;
        self
    }
}

//
// internals
//

/// the node with `target`, if it was built before `node`
// keeps target and visual parent chains acyclic and ordered for single pass layout and paint
pub fn resolve_target<C: Context>(kernel: &Kernel<C>, node: NodeId, target: Id) -> Option<NodeId> {
    let target = kernel.slots.node(target)?;
    (target.index() < node.index()).then_some(target)
}

struct UiInner<'ui, C: Context> {
    kernel: &'ui mut Kernel<C>,
    context: &'ui mut C,
    scratch: &'ui ScratchBuffer,
    node: NodeId,
    // the node this ui acts on, its own unless made by `node`
    target: Id,
    // where this node's call sites start in the frame's site stack or Slots::NONE when not owning a node
    sites: u32,
    // this node in the previous build, and its child to try next
    cursor: Cursor,
}

impl<'ui, C: Context> UiInner<'ui, C> {
    fn new(
        kernel: &'ui mut Kernel<C>,
        context: &'ui mut C,
        scratch: &'ui ScratchBuffer,
        node: NodeId,
        old: u32,
    ) -> Self {
        Self {
            target: kernel.nodes[node.index()].id,
            sites: kernel.sites.len() as u32,
            // wraps to zero for a node new to this build, which has no children to try
            cursor: Cursor {
                old,
                next: old.wrapping_add(1),
            },
            kernel,
            context,
            scratch,
            node,
        }
    }

    #[inline]
    fn borrow(&mut self, target: Id) -> UiInner<'_, C> {
        UiInner {
            kernel: self.kernel,
            context: self.context,
            scratch: self.scratch,
            node: self.node,
            target,
            sites: Slots::NONE,
            cursor: Cursor {
                old: Slots::NONE,
                next: 0,
            },
        }
    }

    // children return their node and its match in the previous build, which fit in one register
    fn child(&mut self, location: &'static Location<'static>) -> (NodeId, u32) {
        // out of line so the usual match stays small
        #[cold]
        fn search(
            old_nodes: &[StoredNode],
            cursor: Cursor,
            end: u32,
            location: &Location<'_>,
            ordinal: u32,
        ) -> Option<u32> {
            // scan past the cursor first so skipped siblings stay linear, then wrap around
            let mut index = if cursor.next <= end {
                old_nodes[cursor.next as usize].subtree_end + 1
            } else {
                end + 1
            };
            while index <= end {
                if old_nodes[index as usize].mark.is_site(location, ordinal) {
                    return Some(index);
                }
                index = old_nodes[index as usize].subtree_end + 1;
            }
            let mut index = cursor.old + 1;
            while index < cursor.next {
                if old_nodes[index as usize].mark.is_site(location, ordinal) {
                    return Some(index);
                }
                index = old_nodes[index as usize].subtree_end + 1;
            }
            None
        }

        let kernel = &mut *self.kernel;
        let start = self.sites as usize;
        let (site, ordinal, scan) = match kernel.sites[start..]
            .iter()
            .rposition(|site| same_place(site.location, location))
        {
            Some(index) => {
                let site = &mut kernel.sites[start + index];
                site.ordinal = site.ordinal.checked_add(1).expect("too many children from one place");
                (start + index, site.ordinal, site.scan)
            }
            None => {
                kernel.sites.push(Site {
                    location,
                    ordinal: 0,
                    scan: true,
                });
                (kernel.sites.len() - 1, 0, true)
            }
        };
        let mut found = None;
        if scan && self.cursor.old != Slots::NONE {
            let end = kernel.old_nodes[self.cursor.old as usize].subtree_end;
            if self.cursor.next <= end
                && kernel.old_nodes[self.cursor.next as usize]
                    .mark
                    .is_site(location, ordinal)
            {
                found = Some(self.cursor.next);
            } else {
                found = search(&kernel.old_nodes, self.cursor, end, location, ordinal);
                kernel.sites[site].scan = found.is_some();
            }
        }
        let (id, old) = match found {
            Some(index) => {
                let stored = &kernel.old_nodes[index as usize];
                self.cursor.next = stored.subtree_end + 1;
                (stored.id, index)
            }
            None => (kernel.slots.alloc(), Slots::NONE),
        };
        (
            kernel.push_node(Some(self.node), id, Mark::Site(location, ordinal)),
            old,
        )
    }

    fn child_in(&mut self, scope: Id, key: u64, location: &'static Location<'static>) -> (NodeId, u32) {
        let kernel = &mut *self.kernel;
        // a scope is kept by the build that made it, while one held from an older build names alone
        let keyed = kernel.slots.is_kept(scope);
        let mut mark = if keyed { Mark::Key(scope.slot, key) } else { Mark::Named };
        let (mut id, mut old) = if keyed
            && self.cursor.old != Slots::NONE
            && self.cursor.next <= kernel.old_nodes[self.cursor.old as usize].subtree_end
            && matches!(kernel.old_nodes[self.cursor.next as usize].mark, Mark::Key(slot, other) if slot == scope.slot && other == key)
        {
            let index = self.cursor.next;
            let stored = &kernel.old_nodes[index as usize];
            self.cursor.next = stored.subtree_end + 1;
            (stored.id, index)
        } else {
            let id = kernel.slots.name(scope, key);
            let old = kernel.slots.old(id).map_or(Slots::NONE, |node| node as u32);
            // a moved key among these siblings keeps the ones after it on the cursor
            if old != Slots::NONE && kernel.old_nodes[old as usize].parent.index() as u32 == self.cursor.old {
                self.cursor.next = kernel.old_nodes[old as usize].subtree_end + 1;
            }
            (id, old)
        };
        if kernel.slots.claimed(id) {
            // the repeat is named alone so the next build finds the first holder
            self.context.duplicate_key(Scope(scope), key, location);
            id = kernel.slots.alloc();
            old = Slots::NONE;
            mark = Mark::Named;
        }
        (kernel.push_node(Some(self.node), id, mark), old)
    }
}

impl<'ui, C: Context, S> Ui<'ui, C, S> {
    pub(crate) fn new(
        kernel: &'ui mut Kernel<C>,
        context: &'ui mut C,
        scratch: &'ui ScratchBuffer,
        node: NodeId,
        old: u32,
    ) -> Self {
        Self {
            inner: UiInner::new(kernel, context, scratch, node, old),
            marker: PhantomData,
        }
    }
}

impl<C: Context> Drop for UiInner<'_, C> {
    fn drop(&mut self) {
        if self.sites == Slots::NONE {
            return;
        }
        self.kernel.nodes[self.node.index()].subtree_end = (self.kernel.nodes.len() - 1) as u32;
        // sites nest like open nodes so this node's are always last
        self.kernel.sites.truncate(self.sites as usize);
    }
}
