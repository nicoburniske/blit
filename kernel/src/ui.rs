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

    /// a target through [`crate::Ui::at`], which may be built later
    pub struct At;

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
        kernel
            .laid_out
            .get(self.inner.target, |index| kernel.old_nodes[index].widget_id)
            .map(|index| kernel.old_placements[index].area)
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
        let id = self.inner.target.child(Location::caller());
        let kernel = &mut *self.inner.kernel;
        // cleared so stored options stay equal across frames and do not retarget
        let initial = options.initial.take().unwrap_or(target);
        let state = AnimationState::update(&mut kernel.animations, id, initial);
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
        let id = self.inner.target.child(Location::caller());
        let kernel = &mut *self.inner.kernel;
        let state = AnimationState::update(&mut kernel.animations, id, 0.0);
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
        let id = self.inner.target.child(Location::caller());
        let kernel = &mut *self.inner.kernel;
        TimerState::update(&mut kernel.timers, id, duration, false, kernel.time)
    }

    /// fires every `period`, counting from when the target first calls it from this site or from a reset
    #[track_caller]
    pub fn interval(&mut self, period: Duration) -> Timer<'_> {
        assert!(!period.is_zero(), "an interval must be nonzero");
        let id = self.inner.target.child(Location::caller());
        let kernel = &mut *self.inner.kernel;
        TimerState::update(&mut kernel.timers, id, period, true, kernel.time)
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
    /// refers to the node `target` names, which may be built later
    ///
    /// a bare key names the child of this node with that key
    pub fn at(&mut self, target: impl Into<Target>) -> Ui<'_, C, state::At> {
        let target = target_id(self.inner.kernel, self.inner.node, target.into());
        Ui {
            inner: UiInner {
                kernel: self.inner.kernel,
                context: self.inner.context,
                scratch: self.inner.scratch,
                node: self.inner.node,
                target,
                sites: self.inner.sites,
                owns_node: false,
            },
            marker: PhantomData,
        }
    }

    /// inserts content into the current node
    #[inline]
    pub fn insert<W: Widget<C, state::Insert>>(&mut self, widget: W) -> W::Response {
        widget.build(Ui {
            inner: UiInner {
                kernel: self.inner.kernel,
                context: self.inner.context,
                scratch: self.inner.scratch,
                node: self.inner.node,
                target: self.inner.target,
                sites: self.inner.sites,
                owns_node: false,
            },
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
        let old_id = |index: usize| kernel.old_nodes[index].widget_id;
        let node = kernel.laid_out.get(self.inner.target, old_id)?;
        let inside = kernel.laid_out.get(focused, old_id)?;
        (node < inside && inside <= kernel.old_nodes[node].subtree_end as usize).then_some(focused)
    }

    /// this node's scope, for descendants to key themselves in with [`Ui::child_in`]
    pub fn scope(&self) -> Scope {
        Scope(self.inner.target)
    }

    /// identifies this node for references within the current render
    pub fn node_id(&self) -> NodeId {
        self.inner.node
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

    pub fn store_mut(&mut self) -> &mut FrameStore
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

    /// places this node against `target` outside its parent's flow
    ///
    /// a target not built before this node places it against its parent
    pub fn place<P: Place<C>>(self, target: impl Into<Target>, place: P) -> Self {
        let kernel = &mut *self.inner.kernel;
        let reference = resolve_target(kernel, self.inner.node, target.into());
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
            let stored = &mut kernel.nodes[node.index()];
            match stored.motion.index() {
                Some(index) => kernel.node_motions[index].motion = motion,
                None => {
                    stored.motion = NodeMotionId::new(kernel.node_motions.len());
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
    pub fn parent(self, target: impl Into<Target>) -> Self {
        if let Some(parent) = resolve_target(self.inner.kernel, self.inner.node, target.into()) {
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
        self.inner.kernel.placements[self.inner.node.index()].z_index = z_index;
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
    /// identifies this child among its siblings by `key`, as [`Ui::child_id`] would
    ///
    /// call it first, as anything added before stays with the previous identity
    #[inline]
    #[track_caller]
    pub fn key(mut self, key: impl Into<Id>) -> Self {
        let kernel = &mut *self.inner.kernel;
        let parent = kernel.nodes[self.inner.node.index()].parent;
        let id = kernel.nodes[parent.index()]
            .widget_id
            .child(TargetKey::Child(key.into()));
        if kernel.watch == Some(id) {
            self.inner.context.duplicate_id(id, Location::caller());
        }
        kernel.nodes[self.inner.node.index()].widget_id = id;
        self.inner.target = id;
        self
    }

    /// sets this child's layout item
    #[inline]
    pub fn item(self, item: I) -> Self {
        let node = &mut self.inner.kernel.nodes[self.inner.node.index()];
        if node.item.offset().is_some() {
            *self.inner.kernel.bump.load_mut(node.item) = item;
        } else {
            node.item = self.inner.kernel.bump.store(item);
        }
        self
    }
}

impl<'ui, C: Context, L: Layout<C>> Ui<'ui, C, state::Open<L>> {
    /// creates a child identified by where it is built
    ///
    /// children built from one place are told apart by the order they are built in
    #[inline(always)]
    #[track_caller]
    pub fn child(&mut self) -> Ui<'_, C, state::Child<L::Item>> {
        let location = Location::caller();
        let kernel = &mut *self.inner.kernel;
        let sites = &mut kernel.sites[self.inner.sites as usize..];
        // copies of one call site may not share an address
        let site = sites.iter_mut().find(|site| {
            core::ptr::eq(site.location, location)
                || (site.location.line() == location.line()
                    && site.location.column() == location.column()
                    && site.location.file() == location.file())
        });
        let id = match site {
            Some(site) => {
                site.count = site.count.checked_add(1).expect("too many children from one place");
                site.id.child(site.count)
            }
            None => {
                // out of line so repeated sites inline into the caller
                #[cold]
                fn first(sites: &mut Vec<Site>, parent: Id, location: &'static Location<'static>) -> Id {
                    let id = parent.child(location);
                    sites.push(Site { location, id, count: 0 });
                    id
                }
                first(&mut kernel.sites, self.inner.target, location)
            }
        };
        let node = kernel.push_node(self.inner.context, Some(self.inner.node), id);
        Ui::new(self.inner.kernel, self.inner.context, self.inner.scratch, node)
    }

    /// creates a child identified among its siblings by `key` rather than where it is built
    ///
    /// [`Target::at`] addresses it
    #[inline]
    #[track_caller]
    pub fn child_id(&mut self, key: impl Into<Id>) -> Ui<'_, C, state::Child<L::Item>> {
        let id = self.inner.target.child(TargetKey::Child(key.into()));
        let kernel = &mut *self.inner.kernel;
        let node = kernel.push_node(self.inner.context, Some(self.inner.node), id);
        Ui::new(self.inner.kernel, self.inner.context, self.inner.scratch, node)
    }

    /// creates a child keyed `key` in `scope`, usually an ancestor's
    ///
    /// it keeps its state when it moves to another parent beneath the scope.
    /// [`Target::scoped`] and [`Scope::key`] address it.
    #[inline]
    #[track_caller]
    pub fn child_in(&mut self, scope: Scope, key: impl Into<Id>) -> Ui<'_, C, state::Child<L::Item>> {
        let id = scope.0.child(TargetKey::Scoped(key.into()));
        let kernel = &mut *self.inner.kernel;
        let node = kernel.push_node(self.inner.context, Some(self.inner.node), id);
        Ui::new(self.inner.kernel, self.inner.context, self.inner.scratch, node)
    }

    /// lays out a built child now and returns its current animated size under `bounds`
    ///
    /// the layout pass lays it out again with whatever bounds this node's layout gives it
    pub fn layout_child(&mut self, child: NodeId, bounds: Constraints<C::Scalar>) -> Size<C::Scalar> {
        let kernel = &mut *self.inner.kernel;
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

/// a node addressed relative to the current one
///
/// a bare key names the child of the current node with that key, and an [`Id`] names its node
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    base: Base,
    key: Option<TargetKey>,
}

impl Target {
    /// the current node
    pub const THIS: Self = Self::new(Base::This);
    /// the current node's structural parent
    pub const PARENT: Self = Self::new(Base::Parent);
    /// the frame root
    pub const ROOT: Self = Self::new(Base::Root);

    /// the child of this node created with [`Ui::child_id`] and `key`
    pub fn at(self, key: impl Into<Id>) -> Self {
        Self {
            key: Some(TargetKey::Child(key.into())),
            ..self
        }
    }

    /// the node created with [`Ui::child_in`] and `key` in this node's [`Scope`]
    pub fn scoped(self, key: impl Into<Id>) -> Self {
        Self {
            key: Some(TargetKey::Scoped(key.into())),
            ..self
        }
    }
}

// a key names a child of the current node
macro_rules! target_from_key {
    ($($key:ty),*) => {$(
        impl From<$key> for Target {
            fn from(key: $key) -> Self {
                Self::THIS.at(key)
            }
        }
    )*};
}

target_from_key!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, &str);

/// the node with this id wherever it is
impl From<Id> for Target {
    fn from(id: Id) -> Self {
        Self::new(Base::Id(id))
    }
}

impl From<NodeId> for Target {
    fn from(node: NodeId) -> Self {
        Self::new(Base::Node(node))
    }
}

/// a node that keys descendants built with [`Ui::child_in`], so they keep their state while
/// moving between parents beneath it
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scope(Id);

impl Scope {
    /// the frame root, whose keys are unique across the frame
    pub const ROOT: Self = Self(ROOT);

    /// the node created with [`Ui::child_in`] and `key` in this scope
    pub fn key(self, key: impl Into<Id>) -> Target {
        Target {
            base: Base::Id(self.0),
            key: Some(TargetKey::Scoped(key.into())),
        }
    }
}

//
// internals
//

pub const ROOT: Id = Id::named(b"blit frame root");

impl Target {
    const fn new(base: Base) -> Self {
        Self { base, key: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Base {
    This,
    Parent,
    Root,
    Node(NodeId),
    Id(Id),
}

// hashed with its base, the variant keeps child and scope keys of one node apart
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TargetKey {
    Child(Id),
    Scoped(Id),
}

// children built from one call site under the open node
pub struct Site {
    location: &'static Location<'static>,
    id: Id,
    count: u32,
}

pub fn target_id<C: Context>(kernel: &Kernel<C>, node: NodeId, target: Target) -> Id {
    let base = match target.base {
        Base::This => kernel.nodes[node.index()].widget_id,
        Base::Parent => kernel.nodes[kernel.nodes[node.index()].parent.index()].widget_id,
        Base::Root => ROOT,
        Base::Node(node) => kernel.nodes[node.index()].widget_id,
        Base::Id(id) => id,
    };
    match target.key {
        Some(key) => base.child(key),
        None => base,
    }
}

/// the node `target` names, if it was built before `node`
pub fn resolve_target<C: Context>(kernel: &Kernel<C>, node: NodeId, target: Target) -> Option<NodeId> {
    let target = match (target.base, target.key) {
        (Base::Parent, None) => kernel.nodes[node.index()].parent,
        (Base::Root, None) => NodeId::new(0),
        (Base::Node(target), None) => target,
        _ => {
            let id = target_id(kernel, node, target);
            // open ancestors are cheap to find on the parent chain
            let mut ancestor = kernel.nodes[node.index()].parent;
            loop {
                let stored = &kernel.nodes[ancestor.index()];
                if stored.widget_id == id {
                    break ancestor;
                }
                if stored.parent == ancestor {
                    // the previous layout's index usually still holds
                    let hint = kernel
                        .laid_out
                        .get(id, |index| kernel.old_nodes[index].widget_id)
                        .filter(|&index| index < node.index() && kernel.nodes[index].widget_id == id);
                    // worst case after the fast paths miss, repeated lookups are n^2
                    let index = hint.or_else(|| {
                        kernel.nodes[..node.index()]
                            .iter()
                            .rposition(|stored| stored.widget_id == id)
                    })?;
                    break NodeId::new(index);
                }
                ancestor = stored.parent;
            }
        }
    };
    (target.index() < node.index()).then_some(target)
}

struct UiInner<'ui, C: Context> {
    kernel: &'ui mut Kernel<C>,
    context: &'ui mut C,
    scratch: &'ui ScratchBuffer,
    node: NodeId,
    // the node this ui acts on, its own unless made by `at`
    target: Id,
    // where this node's call sites start in the frame's site stack
    sites: u32,
    owns_node: bool,
}

impl<'ui, C: Context, S> Ui<'ui, C, S> {
    pub(crate) fn new(
        kernel: &'ui mut Kernel<C>,
        context: &'ui mut C,
        scratch: &'ui ScratchBuffer,
        node: NodeId,
    ) -> Self {
        Self {
            inner: UiInner {
                target: kernel.nodes[node.index()].widget_id,
                sites: kernel.sites.len() as u32,
                kernel,
                context,
                scratch,
                node,
                owns_node: true,
            },
            marker: PhantomData,
        }
    }
}

impl<C: Context> Drop for UiInner<'_, C> {
    fn drop(&mut self) {
        if !self.owns_node {
            return;
        }
        self.kernel.nodes[self.node.index()].subtree_end = (self.kernel.nodes.len() - 1) as u32;
        // sites nest like open nodes so this node's are always last
        self.kernel.sites.truncate(self.sites as usize);
    }
}
