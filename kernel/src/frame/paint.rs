use super::*;

/// context resources and temporary storage for painting
pub struct PaintCx<'a, C: Context> {
    bump: &'a BumpArena,
    context: &'a mut C,
}

impl<'a, C: Context> PaintCx<'a, C> {
    pub fn context(&self) -> &C {
        self.context
    }

    pub fn context_mut(&mut self) -> &mut C {
        self.context
    }

    pub fn scratch(&self) -> &'a ScratchBuffer {
        &self.bump.storage
    }
}

pub fn insert_measure<C: Context, F>(frame: &mut Frame<C>, node: NodeId, measure: F)
where
    F: Fn(&mut C, Constraints<C::Scalar>) -> Size<C::Scalar> + 'static,
{
    fn run<C: Context, F>(
        bump: &BumpArena,
        id: BumpId,
        context: &mut C,
        bounds: Constraints<C::Scalar>,
    ) -> Size<C::Scalar>
    where
        F: Fn(&mut C, Constraints<C::Scalar>) -> Size<C::Scalar> + 'static,
    {
        (bump.load::<F>(id))(context, bounds)
    }

    let id = StoredMeasureId::new(frame.measures.len());
    let node = &mut frame.nodes[node.index()];
    // prepended because measurement combines sizes with max
    frame.measures.push(StoredMeasure {
        run: run::<C, F>,
        bump_id: frame.bump.store(measure),
        next: node.first_measure,
    });
    node.first_measure = id;
}

pub fn insert_paint<C: Context, F>(frame: &mut Frame<C>, node: NodeId, paint: F)
where
    F: Fn(&mut PaintCx<'_, C>, Rect<C::Scalar>) + 'static,
{
    fn run<C: Context, F>(bump: &BumpArena, id: BumpId, context: &mut C, area: Rect<C::Scalar>)
    where
        F: Fn(&mut PaintCx<'_, C>, Rect<C::Scalar>) + 'static,
    {
        (bump.load::<F>(id))(&mut PaintCx { bump, context }, area);
    }

    let id = StoredPaintId::new(frame.paints.len());
    let bump_id = frame.bump.store(paint);
    let node = &mut frame.nodes[node.index()];
    // circular list where last_paint.next points to the first paint
    let next = if let Some(last) = node.last_paint.index() {
        let first = frame.paints[last].next;
        frame.paints[last].next = id;
        first
    } else {
        id
    };
    frame.paints.push(StoredPaint {
        run: run::<C, F>,
        bump_id,
        next,
    });
    node.last_paint = id;
}

pub fn resolve_order<C: Context>(frame: &mut Frame<C>, bump: &BumpArena) {
    frame.paint_order.clear();
    let count = frame.nodes.len();
    let placements = &frame.placements[..count];
    if !frame
        .nodes
        .iter()
        .zip(placements)
        .any(|(node, placement)| placement.z_index != 0 || visual_parent(&frame.aux, node) != node.parent)
    {
        return;
    }

    let mut scratch = bump.storage.slice(count);
    scratch.resize(count, PaintLinks::default());
    let links = &mut scratch[..count];
    // zero marks the end of a child list since the root has no parent
    for index in 1..count {
        let parent = visual_parent(&frame.aux, &frame.nodes[index]).index();
        links[index].next_sibling = links[parent].first_child;
        links[parent].first_child = index as u32;
    }

    // completed nodes fill the front while pending nodes form a stack at the back
    frame.paint_order.resize(count, NodeId::new(0));
    let order = &mut frame.paint_order[..count];
    let mut pending = count - 1;
    for output in 0..count {
        debug_assert!(pending >= output && pending < count);
        let parent = order[pending];
        pending += 1;
        order[output] = parent;
        let start = pending;
        let mut child = links[parent.index()].first_child;
        let mut needs_sort = false;
        while child != 0 {
            let index = child as usize;
            child = links[index].next_sibling;
            needs_sort = needs_sort || placements[index].z_index != 0;
            pending -= 1;
            debug_assert!(pending > output);
            order[pending] = NodeId::new(index);
        }
        let children = &mut order[pending..start];
        if needs_sort {
            children.sort_unstable_by_key(|id| (placements[id.index()].z_index, id.index()));
        }
    }
    debug_assert_eq!(pending, count);
}

pub fn resolve_clips<C: Context>(frame: &mut Frame<C>, bump: &BumpArena) {
    frame.interaction.clips.clear();
    let count = frame.nodes.len();
    let nodes = &mut frame.nodes[..count];
    let placements = &frame.placements[..count];
    let aux = &frame.aux;
    for index in 0..count {
        let parent = if index == 0 {
            ResolvedClipId::NONE
        } else {
            nodes[visual_parent(aux, &nodes[index]).index()].resolved_clip
        };
        nodes[index].resolved_clip = parent;
        let Some(stored) = nodes[index]
            .aux
            .index()
            .map(|index| aux[index].clip)
            .filter(|clip| clip.bump_id.offset().is_some())
        else {
            continue;
        };
        let area = placements[index].area;
        let shape = (frame.clip_kinds[stored.kind as usize].shape)(bump, stored.bump_id, area);
        let parent_bounds = parent
            .index()
            .map_or(frame.screen, |clip| frame.interaction.clips[clip].bounds);
        let bounds = match shape {
            ClipShape::Unbounded => parent_bounds,
            ClipShape::Rect(rect) | ClipShape::RoundedRect { rect, .. } => {
                parent_bounds.intersection(rect).unwrap_or_default()
            }
            ClipShape::Circle { center, radius } => {
                let radius = radius.max(C::Scalar::ZERO);
                let right = parent_bounds.x.endpoint(parent_bounds.width);
                let bottom = parent_bounds.y.endpoint(parent_bounds.height);
                let x = center.x.endpoint(-radius).clamp(parent_bounds.x, right);
                let y = center.y.endpoint(-radius).clamp(parent_bounds.y, bottom);
                let right = center.x.endpoint(radius).clamp(x, right);
                let bottom = center.y.endpoint(radius).clamp(y, bottom);
                Rect::new(x, y, right - x, bottom - y)
            }
        };
        let id = ResolvedClipId::new(frame.interaction.clips.len());
        let depth = parent
            .index()
            .map_or(1, |parent| frame.interaction.clips[parent].depth + 1);
        frame.interaction.clips.push(ResolvedClip {
            parent,
            depth,
            clip: stored,
            area,
            bounds,
            shape,
        });
        nodes[index].resolved_clip = id;
    }
}

pub fn paint<C: Context>(frame: &mut Frame<C>, bump: &BumpArena, context: &mut C) {
    frame.active_clips.clear();
    if frame.paint_order.is_empty() {
        for node in 0..frame.nodes.len() {
            paint_node(frame, bump, context, node);
        }
    } else {
        for index in 0..frame.paint_order.len() {
            let node = frame.paint_order[index].index();
            paint_node(frame, bump, context, node);
        }
    }
    set(frame, bump, context, ResolvedClipId::NONE);
}

fn set<C: Context>(frame: &mut Frame<C>, bump: &BumpArena, context: &mut C, target: ResolvedClipId) {
    let mut common = target;
    while let Some(index) = common.index() {
        let depth = frame.interaction.clips[index].depth as usize;
        if depth <= frame.active_clips.len() && frame.active_clips[depth - 1].0 == common.0 {
            break;
        }
        common = frame.interaction.clips[index].parent;
    }
    while frame.active_clips.last().map_or(u32::MAX, |clip| clip.0) != common.0 {
        let clip = frame.active_clips.pop().unwrap();
        let stored = frame.interaction.clips[clip.index().unwrap()];
        let clip_data = stored.clip;
        (frame.clip_kinds[clip_data.kind as usize].pop)(bump, clip_data.bump_id, context);
    }
    push(frame, bump, context, target, common.0);

    fn push<C: Context>(frame: &mut Frame<C>, bump: &BumpArena, context: &mut C, clip: ResolvedClipId, common: u32) {
        if clip.0 == common {
            return;
        }
        let stored = frame.interaction.clips[clip.index().unwrap()];
        push(frame, bump, context, stored.parent, common);
        let clip_data = stored.clip;
        (frame.clip_kinds[clip_data.kind as usize].push)(bump, clip_data.bump_id, context, stored.area);
        frame.active_clips.push(clip);
    }
}

fn paint_node<C: Context>(frame: &mut Frame<C>, bump: &BumpArena, context: &mut C, node: usize) {
    let Some(last) = frame.nodes[node].last_paint.index() else {
        return;
    };
    let area = frame.placements[node].area;
    let resolved_clip = frame.nodes[node].resolved_clip;
    // painters may draw past their area so renderers cull by what they draw
    let bounds = frame.clip_bounds(resolved_clip);
    if bounds.width <= C::Scalar::ZERO || bounds.height <= C::Scalar::ZERO {
        return;
    }
    set(frame, bump, context, resolved_clip);
    let mut index = frame.paints[last].next.0 as usize;
    loop {
        let stored = frame.paints[index];
        (stored.run)(bump, stored.bump_id, context, area);
        if index == last {
            break;
        }
        index = stored.next.0 as usize;
    }
}

#[derive(Clone, Copy, Default)]
struct PaintLinks {
    first_child: u32,
    next_sibling: u32,
}
