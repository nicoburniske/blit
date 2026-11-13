use super::*;

#[repr(C)]
pub struct LayoutCx<'a, C: Context, I = ()> {
    frame: &'a Frame<C>,
    bump: &'a BumpArena,
    placements: &'a mut [Placement<C::Scalar>],
    context: &'a mut C,
    frame_requested: &'a mut bool,
    animated_start: usize,
    node: NodeId,
    content_origin: Point<C::Scalar>,
    default_item: BumpId,
    item: PhantomData<fn() -> I>,
}

impl<'a, C: Context, I: 'static> LayoutCx<'a, C, I> {
    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn size(&self, child: NodeId) -> Size<C::Scalar> {
        #[cfg(debug_assertions)]
        self.assert_child(child);
        self.placements[child.index()].area.size()
    }

    pub fn measure_atoms(&mut self, constraints: Constraints<C::Scalar>) -> Size<C::Scalar> {
        let node = self.node;
        self.cast::<()>().measure(node, constraints)
    }

    #[inline]
    pub fn children(&self) -> Children<'a> {
        Children {
            nodes: &self.frame.nodes[..=self.frame.nodes[self.node.index()].subtree_end as usize],
            next: NodeId::new(self.node.index() + 1),
        }
    }

    #[inline]
    pub fn item(&self, child: NodeId) -> &'a I {
        let id = self.assert_child(child).item;
        self.bump
            .load(if id.offset().is_some() { id } else { self.default_item })
    }

    #[inline]
    pub fn layout_child(&mut self, child: NodeId, bounds: Constraints<C::Scalar>) -> Size<C::Scalar> {
        self.assert_child(child);
        self.cast::<()>().resolve(child, bounds)
    }

    /// queries a child's exact local dimension without running its layout
    pub fn child_declared_extent(&self, child: NodeId, axis: Axis) -> Option<C::Scalar> {
        let stored = self.assert_child(child);
        if let Some(index) = stored.limits.index() {
            let limits = self.frame.limits[index];
            let min = axis.extent(limits.min);
            let max = axis.extent(limits.max);
            if min == max {
                return Some(min);
            }
        }
        None
    }

    pub fn set_child_position(&mut self, child: NodeId, position: Point<C::Scalar>) {
        #[cfg(debug_assertions)]
        self.assert_child(child);
        let area = &mut self.placements[child.index()].area;
        area.x = position.x.endpoint(self.content_origin.x);
        area.y = position.y.endpoint(self.content_origin.y);
    }

    pub fn request_frame(&mut self) {
        *self.frame_requested = true;
    }

    pub fn context(&self) -> &C {
        self.context
    }

    pub fn context_mut(&mut self) -> &mut C {
        self.context
    }

    pub fn scratch(&self) -> &'a ScratchBuffer {
        &self.bump.storage
    }

    pub fn set_child_z_index(&mut self, child: NodeId, z_index: i16) {
        #[cfg(debug_assertions)]
        self.assert_child(child);
        self.placements[child.index()].z_index = z_index;
    }

    pub fn size_overrides(&self, child: NodeId) -> (Option<C::Scalar>, Option<C::Scalar>) {
        let Some(&animated) = child
            .index()
            .checked_sub(self.animated_start)
            .and_then(|index| self.frame.animated_sizes.get(index))
        else {
            return (None, None);
        };
        let current = self.placements[child.index()].area.size();
        (
            animated.intersects(MotionProps::WIDTH).then_some(current.width),
            animated.intersects(MotionProps::HEIGHT).then_some(current.height),
        )
    }
}

/// resolves an out of flow node after all flow layout
pub struct PlaceCx<'a, C: Context> {
    layout: LayoutCx<'a, C>,
    node: NodeId,
    parent: Size<C::Scalar>,
}

impl<C: Context> PlaceCx<'_, C> {
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// the visual parent's size
    pub fn parent(&self) -> Size<C::Scalar> {
        self.parent
    }

    pub fn screen(&self) -> Rect<C::Scalar> {
        self.layout.frame.screen
    }

    /// lays out this node and returns its current size
    pub fn resolve(&mut self, bounds: Constraints<C::Scalar>) -> Size<C::Scalar> {
        self.layout.resolve(self.node, bounds)
    }

    pub fn request_frame(&mut self) {
        self.layout.request_frame();
    }

    pub fn context(&self) -> &C {
        self.layout.context
    }

    pub fn context_mut(&mut self) -> &mut C {
        self.layout.context
    }
}

#[derive(Clone, Copy)]
pub struct Children<'a> {
    nodes: &'a [StoredNode],
    next: NodeId,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    #[inline]
    fn next(&mut self) -> Option<NodeId> {
        while (self.next.value as usize) < self.nodes.len() {
            let node = self.next;
            let stored = &self.nodes[node.index()];
            self.next.value = stored.subtree_end + 1;
            if stored.place.index().is_none() {
                return Some(node);
            }
        }
        None
    }
}

//
// internals
//
impl<'a, C: Context, I: 'static> LayoutCx<'a, C, I> {
    #[track_caller]
    fn assert_child(&self, child: NodeId) -> &'a StoredNode {
        let stored = &self.frame.nodes[child.index()];
        assert!(
            child != self.node && stored.parent == self.node && stored.place.index().is_none(),
            "layout can only access direct flow children"
        );
        stored
    }

    #[inline]
    fn cast<J>(&mut self) -> &mut LayoutCx<'a, C, J> {
        // safety: repr(C) changes only the item marker and preserves this borrow and lifetime
        unsafe { &mut *std::ptr::from_mut(self).cast::<LayoutCx<'a, C, J>>() }
    }
}

impl<'a, C: Context> LayoutCx<'a, C> {
    /// measures atoms against the inner box
    #[inline]
    fn measure(&mut self, node: NodeId, constraints: Constraints<C::Scalar>) -> Size<C::Scalar> {
        if constraints.min == constraints.max {
            return constraints.min;
        }
        let stored = &self.frame.nodes[node.index()];
        let mut atom = stored.first_atom;
        let mut size = Size::ZERO;
        let inset = stored.edges.index().map(|index| self.frame.edges[index].inset);
        let Some(inset) = inset.filter(|inset| *inset != Sides::all(C::Scalar::ZERO)) else {
            while let Some(index) = atom.index() {
                let stored = self.frame.atoms[index];
                let measure = stored.kind().measure;
                size = size.max(measure(self.bump, stored.bump_id, self.context, constraints));
                atom = stored.next;
            }
            return constraints.constrain(size);
        };
        let mut remaining = inset.size();
        while let Some(index) = atom.index() {
            let stored = self.frame.atoms[index];
            let kind = stored.kind();
            let (measure, atom_inset) = (kind.measure, kind.inset);
            let grow = |size: Size<C::Scalar>| {
                Size::new(
                    size.width.endpoint(remaining.width),
                    size.height.endpoint(remaining.height),
                )
            };
            let bounds = Constraints {
                min: grow(constraints.min),
                max: grow(constraints.max),
            };
            let measured = measure(self.bump, stored.bump_id, self.context, bounds);
            size = size.max((measured - remaining).max(Size::ZERO));
            remaining = remaining - atom_inset(self.bump, stored.bump_id).size();
            atom = stored.next;
        }
        constraints.constrain(size)
    }

    #[inline]
    fn resolve(&mut self, node: NodeId, mut bounds: Constraints<C::Scalar>) -> Size<C::Scalar> {
        if let Some(index) = self.frame.nodes[node.index()].limits.index() {
            let limits = self.frame.limits[index];
            bounds = Constraints {
                min: bounds.constrain(limits.min),
                max: bounds.constrain(limits.max),
            };
        }
        let (width, height) = self.size_overrides(node);
        if let Some(width) = width {
            bounds.min.width = width;
            bounds.max.width = width;
        }
        if let Some(height) = height {
            bounds.min.height = height;
            bounds.max.height = height;
        }
        let stored = &self.frame.nodes[node.index()];
        let outer = bounds;
        let inset = stored
            .edges
            .index()
            .map(|index| self.frame.edges[index].inset)
            .filter(|inset| *inset != Sides::all(C::Scalar::ZERO));
        let (leading, bounds) = match inset {
            Some(inset) => (
                Point::new(inset.left.min(bounds.max.width), inset.top.min(bounds.max.height)),
                bounds.shrink(inset.size()),
            ),
            None => (Point::ZERO, bounds),
        };
        let size = if stored.layout.offset().is_some() {
            let kind = &self.frame.layout_kinds[stored.layout_kind as usize];
            let mut cx = LayoutCx {
                frame: self.frame,
                bump: self.bump,
                placements: self.placements,
                context: self.context,
                frame_requested: self.frame_requested,
                animated_start: self.animated_start,
                node,
                content_origin: leading,
                default_item: kind.default_item,
                item: PhantomData,
            };
            for child in cx.children() {
                let area = &mut cx.placements[child.index()].area;
                area.x = leading.x;
                area.y = leading.y;
            }
            (kind.run)(&mut cx, stored.layout, bounds)
        } else {
            self.measure(node, bounds)
        };
        debug_assert_eq!(
            size,
            bounds.constrain(size),
            "layout returned a size outside its constraints"
        );
        let size = inset.map_or(size, |inset| {
            let inset = inset.size();
            outer.constrain(Size::new(
                size.width.endpoint(inset.width),
                size.height.endpoint(inset.height),
            ))
        });
        let area = &mut self.placements[node.index()].area;
        area.width = size.width;
        area.height = size.height;
        size
    }
}

/// resolves layout motion against a subtree's target layout
///
/// - layout first establishes final areas
/// - size animation updates placements and replays layout
/// - position animation applies after layout without replay
pub fn resolve<C: Context>(
    frame: &mut Frame<C>,
    bump: &BumpArena,
    context: &mut C,
    root: NodeId,
    bounds: Constraints<C::Scalar>,
    commit: bool,
) -> Size<C::Scalar> {
    let mut size = arrange(frame, bump, context, root, bounds);
    if frame.bounds.is_empty() {
        return size;
    }
    let start = root.index();
    let end = frame.nodes[start].subtree_end as usize + 1;
    let nodes = &frame.nodes[..end];
    let mut index = start;
    while index < end {
        let stored = &nodes[index];
        if !commit && start != 0 && stored.place.index().is_some() {
            index = stored.subtree_end as usize + 1;
            continue;
        }
        index += 1;
        let Some(bounds) = stored.bounds.index() else {
            continue;
        };
        let record = frame.bounds[bounds];
        let Some(mut config) = record.motion else { continue };
        if !commit && !config.properties.intersects(MotionProps::SIZE) {
            continue;
        }
        let node = record.node;
        let id = stored.widget_id;
        if stored.parent == node {
            config.properties = config
                .properties
                .intersection(MotionProps::POSITION | MotionProps::RESIZE);
        }
        let found = frame.motions.binary_search_by_key(&id, |state| state.id);
        let mut initial = MotionState::new(id, node);
        let state = match found {
            Ok(index) => &mut frame.motions[index],
            Err(_) => &mut initial,
        };
        let (values, animated) = state.advance(
            frame.placements[node.index()].area,
            config,
            frame.time,
            frame.resized,
            commit,
        );
        if commit {
            state.node = node;
            state.properties = config.properties;
            state.seen = true;
        }
        let properties = animated.intersection(MotionProps::SIZE);
        if !properties.is_empty() {
            if frame.animated_sizes.is_empty() {
                frame.animated_sizes.resize(end - start, MotionProps::NONE);
            }
            let area = &mut frame.placements[node.index()].area;
            if properties.intersects(MotionProps::WIDTH) {
                area.width = values[2];
            }
            if properties.intersects(MotionProps::HEIGHT) {
                area.height = values[3];
            }
            frame.animated_sizes[node.index() - start] = properties;
        }
        if let Err(index) = found
            && commit
        {
            frame.motions.insert(index, initial);
        }
    }
    if !frame.animated_sizes.is_empty() {
        size = arrange(frame, bump, context, root, bounds);
        frame.animated_sizes.clear();
    }
    // settled position channels still override reflow from animated sizes
    if commit {
        for state in frame.motions.iter().filter(|state| state.seen) {
            let area = &mut frame.placements[state.node.index()].area;
            if state.properties.intersects(MotionProps::X) {
                area.x = state.animator.values[0];
            }
            if state.properties.intersects(MotionProps::Y) {
                area.y = state.animator.values[1];
            }
        }
    }
    size
}

/// offsets each area by its target
pub fn position<C: Context>(frame: &mut Frame<C>) {
    let count = frame.nodes.len();
    let placements = &mut frame.placements[..count];
    for index in 1..count {
        let reference = frame.nodes[index].target;
        let reference = placements[reference.index()].area;
        let area = &mut placements[index].area;
        area.x += reference.x;
        area.y += reference.y;
    }
}

pub fn place<C: Context, P: Place<C>>(
    cx: &mut PlaceCx<'_, C>,
    id: BumpId,
    target: Rect<C::Scalar>,
) -> Point<C::Scalar> {
    let bump = cx.layout.bump;
    bump.load::<P>(id).place(cx, target)
}

pub fn run<'a, C: Context, L: Layout<C>>(
    cx: &mut LayoutCx<'a, C>,
    id: BumpId,
    bounds: Constraints<C::Scalar>,
) -> Size<C::Scalar> {
    let layout = cx.bump.load::<L>(id);
    layout.layout(cx.cast::<L::Item>(), bounds)
}

fn arrange<C: Context>(
    frame: &mut Frame<C>,
    bump: &BumpArena,
    context: &mut C,
    node: NodeId,
    bounds: Constraints<C::Scalar>,
) -> Size<C::Scalar> {
    let mut placements = std::mem::take(&mut frame.placements);
    let mut requested = frame.frame_requested;
    let mut cx = LayoutCx {
        frame,
        bump,
        placements: &mut placements,
        context,
        frame_requested: &mut requested,
        animated_start: node.index(),
        node,
        content_origin: Point::ZERO,
        default_item: BumpId::NONE,
        item: PhantomData::<fn() -> ()>,
    };
    let size = cx.resolve(node, bounds);
    if node.value == 0 {
        for index in 1..cx.frame.nodes.len() {
            let stored = &cx.frame.nodes[index];
            let Some(place) = stored.place.index() else {
                continue;
            };
            let place = cx.frame.places[place];
            // earlier areas are resolved and relative to their targets
            let mut reference = stored.target;
            let mut origin = Point::ZERO;
            loop {
                let area = cx.placements[reference.index()].area;
                origin = Point::new(origin.x + area.x, origin.y + area.y);
                let next = cx.frame.nodes[reference.index()].target;
                if next == reference {
                    break;
                }
                reference = next;
            }
            let size = cx.placements[stored.target.index()].area.size();
            let parent = cx.placements[stored.visual_parent.index()].area.size();
            let mut place_cx = PlaceCx {
                layout: LayoutCx {
                    placements: &mut *cx.placements,
                    context: &mut *cx.context,
                    frame_requested: &mut *cx.frame_requested,
                    ..cx
                },
                node: NodeId::new(index),
                parent,
            };
            let target = Rect::new(origin.x, origin.y, size.width, size.height);
            let position = (place.run)(&mut place_cx, place.bump_id, target);
            let area = &mut cx.placements[index].area;
            area.x = position.x - origin.x;
            area.y = position.y - origin.y;
        }
    }
    frame.placements = placements;
    frame.frame_requested = requested;
    size
}
