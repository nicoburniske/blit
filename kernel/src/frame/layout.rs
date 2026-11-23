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
        self.placements[child.index()].area.size()
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
        let stored = &self.frame.nodes[child.index()];
        // other nodes may store items of another type
        assert_eq!(stored.parent, self.node, "layout items belong to direct children");
        let id = stored.item;
        self.bump
            .load(if id.offset().is_some() { id } else { self.default_item })
    }

    #[inline]
    pub fn layout_child(&mut self, child: NodeId, bounds: Constraints<C::Scalar>) -> Size<C::Scalar> {
        self.cast::<()>().resolve(child, bounds)
    }

    /// the extent a child resolves to regardless of bounds from motion or exact limits
    pub fn fixed_extent(&self, child: NodeId, axis: Axis) -> Option<C::Scalar> {
        let stored = &self.frame.nodes[child.index()];
        axis.extent(self.size_overrides(child)).or_else(|| {
            let limits = self.frame.aux[stored.aux.index()?].limits;
            let min = axis.extent(limits.min);
            (min == axis.extent(limits.max)).then_some(min)
        })
    }

    pub fn set_child_position(&mut self, child: NodeId, position: Point<C::Scalar>) {
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
        self.placements[child.index()].z_index = z_index;
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
        while (self.next.0 as usize) < self.nodes.len() {
            let node = self.next;
            let stored = &self.nodes[node.index()];
            self.next.0 = stored.subtree_end + 1;
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
    #[inline]
    fn cast<J>(&mut self) -> &mut LayoutCx<'a, C, J> {
        // safety: repr(C) changes only the item marker and preserves this borrow and lifetime
        unsafe { &mut *core::ptr::from_mut(self).cast::<LayoutCx<'a, C, J>>() }
    }

    fn size_overrides(&self, node: NodeId) -> Size<Option<C::Scalar>> {
        let Some(&animated) = node
            .index()
            .checked_sub(self.animated_start)
            .and_then(|index| self.frame.animated_sizes.get(index))
        else {
            return Size::new(None, None);
        };
        let current = self.placements[node.index()].area.size();
        Size::new(
            animated.intersects(MotionProps::WIDTH).then_some(current.width),
            animated.intersects(MotionProps::HEIGHT).then_some(current.height),
        )
    }
}

impl<'a, C: Context> LayoutCx<'a, C> {
    /// measures content against the full box
    #[inline]
    fn measure(&mut self, node: NodeId, constraints: Constraints<C::Scalar>) -> Size<C::Scalar> {
        if constraints.min == constraints.max {
            return constraints.min;
        }
        let mut measure = self.frame.nodes[node.index()].first_measure;
        let mut size = Size::ZERO;
        while let Some(index) = measure.index() {
            let stored = self.frame.measures[index];
            size = size.max((stored.run)(self.bump, stored.bump_id, self.context, constraints));
            measure = stored.next;
        }
        constraints.constrain(size)
    }

    #[inline]
    fn resolve(&mut self, node: NodeId, mut bounds: Constraints<C::Scalar>) -> Size<C::Scalar> {
        if let Some(index) = self.frame.nodes[node.index()].aux.index() {
            let limits = self.frame.aux[index].limits;
            // a node's own limits win over its parent's bounds
            bounds = Constraints {
                min: limits.constrain(bounds.min),
                max: limits.constrain(bounds.max),
            };
        }
        let Size { width, height } = self.size_overrides(node);
        if let Some(width) = width {
            bounds.min.width = width;
            bounds.max.width = width;
        }
        if let Some(height) = height {
            bounds.min.height = height;
            bounds.max.height = height;
        }
        let stored = &self.frame.nodes[node.index()];
        // content sizes every node so layouts only account for children
        let mut size = self.measure(node, bounds);
        if stored.layout.offset().is_some() {
            let padding = stored
                .aux
                .index()
                .map_or(Sides::all(C::Scalar::ZERO), |index| self.frame.aux[index].padding);
            let leading = Point::new(padding.left.min(bounds.max.width), padding.top.min(bounds.max.height));
            let inner = bounds.shrink(padding.size());
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
            let content = (kind.run)(&mut cx, stored.layout, inner);
            debug_assert_eq!(
                content,
                inner.constrain(content),
                "layout returned a size outside its constraints"
            );
            let padding = padding.size();
            size = size.max(bounds.constrain(Size::new(
                content.width.endpoint(padding.width),
                content.height.endpoint(padding.height),
            )));
        }
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
    if frame.node_motions.is_empty() {
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
        let Some(record) = stored.motion.index() else {
            continue;
        };
        let NodeMotion {
            node,
            motion: mut config,
        } = frame.node_motions[record];
        if !commit && !config.properties.intersects(MotionProps::SIZE) {
            continue;
        }
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
        let stored = &frame.nodes[index];
        let reference = placements[stored.target.index()].area;
        let area = &mut placements[index].area;
        area.x += reference.x;
        area.y += reference.y;
        let Some(record) = stored.motion.index() else {
            continue;
        };
        let motion = frame.node_motions[record].motion;
        // a node that changes reference continues from where it was on screen and its subtree follows
        let id = stored.widget_id;
        let Some(old) = frame.laid_out.get(id, |index| frame.old_nodes[index].widget_id) else {
            continue;
        };
        // previous nodes belong to another build, so their references are read by value
        let old_reference = frame.old_nodes[frame.old_nodes[old].target.0 as usize].widget_id;
        if old_reference == frame.nodes[stored.target.index()].widget_id {
            continue;
        }
        let Ok(state) = frame.motions.binary_search_by_key(&id, |state| state.id) else {
            continue;
        };
        frame.motions[state].flying = true;
        let animator = &mut frame.motions[state].animator;
        let start = frame.old_placements[old].area;
        if motion.properties.intersects(MotionProps::X) && area.x != start.x {
            animator.displace(0, area.x.delta(start.x), frame.time);
            area.x = start.x;
        }
        if motion.properties.intersects(MotionProps::Y) && area.y != start.y {
            animator.displace(1, area.y.delta(start.y), frame.time);
            area.y = start.y;
        }
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
    let mut placements = core::mem::take(&mut frame.placements);
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
    if node.0 == 0 {
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
            let parent = cx.placements[visual_parent(&cx.frame.aux, stored).index()].area.size();
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
