use super::*;

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
    pub trait NoLayout {}

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

    /// returns the target's area from the previous layout and tracks it for the next one
    pub fn area(&mut self) -> Option<Rect<C::Scalar>> {
        let frame = &mut *self.inner.frame;
        let id = self.inner.target;
        frame.areas_requested.push(id);
        let areas = &frame.areas_previous;
        areas
            .binary_search_by_key(&id, |(candidate, _)| *candidate)
            .ok()
            .map(|index| areas[index].1)
    }

    /// adds `interaction` to the target and returns its latest response
    ///
    /// one interaction of each kind per target, repeated requests share it
    pub fn interact<K: Interact<C>>(&mut self, interaction: K) -> K::Response {
        let frame = &mut *self.inner.frame;
        frame
            .interaction
            .interact::<C, K>(self.inner.target, interaction, frame.time)
    }

    /// animates toward `target`, keyed by the target and call site
    #[track_caller]
    pub fn animate(&mut self, target: f32, animation: impl Into<Animation>, mut options: AnimationOptions) -> f32 {
        let id = self.inner.target.child(Location::caller());
        let frame = &mut *self.inner.frame;
        // cleared so stored options stay equal across frames and do not retarget
        let initial = options.initial.take().unwrap_or(target);
        let state = AnimationState::update(&mut frame.animations, id, initial);
        match &mut state.playback {
            Playback::Target(animator) => {
                animator
                    .advance([target], [true], animation.into(), options, frame.time, true)
                    .0[0]
            }
            Playback::Loop { value, .. } => {
                let mut animator = Animator::new([*value], Animation::default(), AnimationOptions::default());
                let value = animator
                    .advance([target], [true], animation.into(), options, frame.time, true)
                    .0[0];
                state.playback = Playback::Target(animator);
                value
            }
        }
    }

    #[track_caller]
    pub fn animate_loop(&mut self, tween: Tween) -> f32 {
        if tween.duration.is_zero() {
            return self.animate(0.0, Animation::default(), AnimationOptions::default());
        }
        let id = self.inner.target.child(Location::caller());
        let frame = &mut *self.inner.frame;
        let state = AnimationState::update(&mut frame.animations, id, 0.0);
        let now = frame.time.as_secs_f64();
        let time = match state.playback {
            Playback::Loop {
                tween: previous, time, ..
            } if previous == tween => time,
            _ => now,
        };
        let progress = (now - time).max(0.0) as f32 / tween.duration.as_secs_f32();
        let value = tween.easing.sample(progress % 1.0).0;
        state.playback = Playback::Loop { tween, time, value };
        value
    }

    /// fires once, `duration` after the target first calls it from this site
    #[track_caller]
    pub fn timer(&mut self, duration: Duration) -> bool {
        let id = self.inner.target.child(Location::caller());
        let frame = &mut *self.inner.frame;
        TimerState::update(&mut frame.timers, id, duration, None, frame.time)
    }

    /// fires every `period`, counting from when the target first calls it from this site
    #[track_caller]
    pub fn interval(&mut self, period: Duration) -> bool {
        assert!(!period.is_zero(), "an interval must be nonzero");
        let id = self.inner.target.child(Location::caller());
        let frame = &mut *self.inner.frame;
        TimerState::update(&mut frame.timers, id, period, Some(period), frame.time)
    }

    /// focuses the target, which lasts while it requests an interaction
    pub fn focus(&mut self) {
        self.inner.frame.interaction.focused = Some(self.inner.target);
    }

    /// whether the target holds focus
    pub fn focused(&self) -> bool {
        self.inner.frame.interaction.focused == Some(self.inner.target)
    }

    pub fn clear_focus(&mut self) {
        self.inner.frame.interaction.focused = None;
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
        self.inner.frame.interaction.mouse_position
    }

    /// the position of a contact in this frame
    ///
    /// ended and cancelled contacts remain available until the next build
    pub fn contact_position(&self, id: ContactId) -> Option<Point<C::Scalar>> {
        self.inner.frame.interaction.contact_position(id)
    }

    /// this frame's input regardless of focus or claims
    ///
    /// handle keyboard shortcuts through an interaction to respect input routing.
    pub fn input(&self) -> Input<C::Scalar> {
        self.inner.frame.input
    }

    pub fn screen(&self) -> Rect<C::Scalar> {
        self.inner.frame.screen
    }

    pub fn time(&self) -> Duration {
        self.inner.frame.time
    }

    pub fn request_frame(&mut self) {
        self.inner.frame.request_frame();
    }
}

impl<'ui, C: Context, S: state::Local> Ui<'ui, C, S> {
    /// refers to the node `target` names, which may be built later
    ///
    /// a bare key names the child of this node with that key
    pub fn at(&mut self, target: impl Into<Target>) -> Ui<'_, C, state::At> {
        let target = self.inner.frame.target_id(self.inner.node, target.into());
        Ui {
            inner: UiInner {
                frame: self.inner.frame,
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
    ///
    /// layouts choose whether their atoms contribute to sizing
    #[inline]
    pub fn insert<T, W>(&mut self, widget: T) -> W::Response
    where
        T: IntoWidget<C, W>,
        W: Widget<C, state::Insert>,
    {
        widget.into_widget().build(Ui {
            inner: UiInner {
                frame: self.inner.frame,
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
        let frame = &mut *self.inner.frame;
        let node = &mut frame.nodes[self.inner.node.index()];
        if let Some(index) = node.limits.index() {
            frame.limits[index] = limits;
        } else {
            node.limits = LimitsId::new(frame.limits.len());
            frame.limits.push(limits);
        }
        self
    }

    /// places this node against `target` outside its parent's flow
    pub fn place<P: Place<C>>(self, target: impl Into<Target>, place: P) -> Self {
        let frame = &mut *self.inner.frame;
        let reference = frame.resolve_target(self.inner.node, target.into());
        let id = PlaceId::new(frame.places.len());
        frame.places.push(StoredPlace {
            bump_id: frame.bump.store(place),
            run: layout::place::<C, P>,
        });
        let node = &mut frame.nodes[self.inner.node.index()];
        node.target = reference;
        node.place = id;
        self
    }

    pub fn motion(self, motion: Motion) -> Self {
        self.inner.frame.bounds_mut(self.inner.node).motion = Some(motion);
        self
    }

    /// selects the parent for stacking, clipping and containing size
    ///
    /// the target must already exist. positioning stays with its anchor.
    #[inline]
    pub fn parent(self, target: impl Into<Target>) -> Self {
        let parent = self.inner.frame.resolve_target(self.inner.node, target.into());
        self.inner.frame.nodes[self.inner.node.index()].visual_parent = parent;
        self
    }

    pub fn clip<X: Clip<C>>(self, clip: X) -> Self {
        let clip = self.inner.frame.store_clip(clip);
        self.inner.frame.nodes[self.inner.node.index()].clip = clip;
        self
    }

    /// sets this node's paint order among its visual siblings
    pub fn z_index(self, z_index: i16) -> Self {
        self.inner.frame.placements[self.inner.node.index()].z_index = z_index;
        self
    }

    pub fn hit(self, hit: Sides<C::Scalar>) -> Self {
        self.inner.frame.bounds_mut(self.inner.node).hit = hit;
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

    /// builds a flow subtree and measures its current animated size under `bounds`
    pub fn build_measured<W: Widget<C>>(
        self,
        bounds: Constraints<C::Scalar>,
        widget: W,
    ) -> (W::Response, Size<C::Scalar>) {
        let mut inner = self.inner;
        inner.owns_node = false;
        let node = inner.node;
        let response = widget.build(Ui::new(inner.frame, inner.context, inner.scratch, node));
        assert!(
            inner.frame.nodes[node.index()].place.index().is_none(),
            "measured widgets must stay in flow"
        );
        let mut bump = std::mem::take(&mut inner.frame.bump);
        bump.storage.prepare();
        let size = layout::resolve(inner.frame, &bump, inner.context, node, bounds, false);
        inner.frame.bump = bump;
        (response, size)
    }

    /// establishes the current node's layout
    pub fn layout<L: Layout<C>>(self, layout: L) -> Ui<'ui, C, state::Open<L>> {
        let Ui { inner, .. } = self;
        inner.frame.store_layout(inner.node, layout);
        Ui {
            inner,
            marker: PhantomData,
        }
    }
}

impl<'ui, C: Context, I: 'static> Ui<'ui, C, state::Child<I>> {
    /// sets this child's layout item
    #[inline]
    pub fn item(self, item: I) -> Self {
        let node = &mut self.inner.frame.nodes[self.inner.node.index()];
        if node.item.offset().is_some() {
            *self.inner.frame.bump.load_mut(node.item) = item;
        } else {
            node.item = self.inner.frame.bump.store(item);
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
        let frame = &mut *self.inner.frame;
        let sites = &mut frame.sites[self.inner.sites as usize..];
        // copies of one call site may not share an address
        let site = sites.iter_mut().find(|site| {
            std::ptr::eq(site.location, location)
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
                first(&mut frame.sites, self.inner.target, location)
            }
        };
        let node = frame.push_node(Some(self.inner.node), id);
        Ui::new(self.inner.frame, self.inner.context, self.inner.scratch, node)
    }

    /// creates a child identified among its siblings by `key` rather than where it is built
    ///
    /// [`Target::at`] addresses it
    #[inline]
    pub fn child_id(&mut self, key: impl Into<Id>) -> Ui<'_, C, state::Child<L::Item>> {
        let id = self.inner.target.child(Key::Child(key.into()));
        let node = self.inner.frame.push_node(Some(self.inner.node), id);
        Ui::new(self.inner.frame, self.inner.context, self.inner.scratch, node)
    }

    /// creates a child keyed `key` in `scope`, usually an ancestor's
    ///
    /// it keeps its state when it moves to another parent beneath the scope.
    /// [`Target::scoped`] and [`Scope::key`] address it.
    #[inline]
    pub fn child_in(&mut self, scope: Scope, key: impl Into<Id>) -> Ui<'_, C, state::Child<L::Item>> {
        let id = scope.0.child(Key::Scoped(key.into()));
        let node = self.inner.frame.push_node(Some(self.inner.node), id);
        Ui::new(self.inner.frame, self.inner.context, self.inner.scratch, node)
    }

    /// insets this node's children while atoms keep the full box
    pub fn pad(self, padding: impl Into<Sides<C::Scalar>>) -> Self {
        let padding = padding.into();
        assert!(
            [padding.top, padding.right, padding.bottom, padding.left]
                .iter()
                .all(|side| *side >= C::Scalar::ZERO && side.is_finite()),
            "invalid padding"
        );
        let frame = &mut *self.inner.frame;
        let node = &mut frame.nodes[self.inner.node.index()];
        if let Some(index) = node.padding.index() {
            frame.paddings[index] = padding;
        } else {
            node.padding = PaddingId::new(frame.paddings.len());
            frame.paddings.push(padding);
        }
        self
    }
}

impl<C: Context, A: Atom<C>> Widget<C, state::Insert> for AtomWidget<A> {
    type Response = ();

    #[inline]
    fn build(self, ui: Ui<'_, C, state::Insert>) {
        atom::insert(ui.inner.frame, ui.inner.node, self.0);
    }
}

/// a node addressed relative to the current one
///
/// a bare key names the child of the current node with that key
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    base: Base,
    key: Option<Key>,
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
            key: Some(Key::Child(key.into())),
            ..self
        }
    }

    /// the node created with [`Ui::child_in`] and `key` in this node's [`Scope`]
    pub fn scoped(self, key: impl Into<Id>) -> Self {
        Self {
            key: Some(Key::Scoped(key.into())),
            ..self
        }
    }
}

impl<K: Into<Id>> From<K> for Target {
    fn from(key: K) -> Self {
        Self::THIS.at(key)
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
            key: Some(Key::Scoped(key.into())),
        }
    }
}

//
// internals
//

pub(super) const ROOT: Id = Id::named(b"blit frame root");

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
enum Key {
    Child(Id),
    Scoped(Id),
}

// children built from one call site under the open node
pub(super) struct Site {
    location: &'static Location<'static>,
    id: Id,
    count: u32,
}

impl<C: Context> Frame<C> {
    pub(super) fn target_id(&self, node: NodeId, target: Target) -> Id {
        let base = match target.base {
            Base::This => self.nodes[node.index()].widget_id,
            Base::Parent => self.nodes[self.nodes[node.index()].parent.index()].widget_id,
            Base::Root => ROOT,
            Base::Node(node) => self.nodes[node.index()].widget_id,
            Base::Id(id) => id,
        };
        match target.key {
            Some(key) => base.child(key),
            None => base,
        }
    }

    fn resolve_target(&self, node: NodeId, target: Target) -> NodeId {
        let target = match (target.base, target.key) {
            (Base::Parent, None) => self.nodes[node.index()].parent,
            (Base::Root, None) => NodeId::new(0),
            (Base::Node(target), None) => target,
            _ => {
                let id = self.target_id(node, target);
                // open ancestors are cheap to find on the parent chain
                let mut ancestor = self.nodes[node.index()].parent;
                loop {
                    let stored = &self.nodes[ancestor.index()];
                    if stored.widget_id == id {
                        break ancestor;
                    }
                    if stored.parent == ancestor {
                        // search recent closed nodes first
                        // todo: repeated lookups are n^2
                        let index = self.nodes[..node.index()]
                            .iter()
                            .rposition(|stored| stored.widget_id == id)
                            .expect("target must already be built");
                        break NodeId::new(index);
                    }
                    ancestor = stored.parent;
                }
            }
        };
        assert!(target.index() < node.index(), "target must be declared before its node");
        target
    }
}

struct UiInner<'ui, C: Context> {
    frame: &'ui mut Frame<C>,
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
    pub(super) fn new(
        frame: &'ui mut Frame<C>,
        context: &'ui mut C,
        scratch: &'ui ScratchBuffer,
        node: NodeId,
    ) -> Self {
        Self {
            inner: UiInner {
                target: frame.nodes[node.index()].widget_id,
                sites: frame.sites.len() as u32,
                frame,
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
        self.frame.nodes[self.node.index()].subtree_end = (self.frame.nodes.len() - 1) as u32;
        // sites nest like open nodes so this node's are always last
        self.frame.sites.truncate(self.sites as usize);
    }
}
