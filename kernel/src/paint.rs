use crate::*;

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

//
// internals
//

pub fn insert_measure<C: Context, F>(kernel: &mut Kernel<C>, node: NodeId, measure: F)
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

    let id = StoredMeasureId::new(kernel.measures.len());
    let node = &mut kernel.nodes[node.index()];
    // prepended because measurement combines sizes with max
    kernel.measures.push(StoredMeasure {
        run: run::<C, F>,
        bump_id: kernel.bump.store(measure),
        next: node.first_measure,
    });
    node.first_measure = id;
}

pub fn insert_paint<C: Context, F>(kernel: &mut Kernel<C>, node: NodeId, paint: F)
where
    F: Fn(&mut PaintCx<'_, C>, Rect<C::Scalar>) + 'static,
{
    fn run<C: Context, F>(bump: &BumpArena, id: BumpId, context: &mut C, area: Rect<C::Scalar>)
    where
        F: Fn(&mut PaintCx<'_, C>, Rect<C::Scalar>) + 'static,
    {
        (bump.load::<F>(id))(&mut PaintCx { bump, context }, area);
    }

    let id = StoredPaintId::new(kernel.paints.len());
    let bump_id = kernel.bump.store(paint);
    let node = &mut kernel.nodes[node.index()];
    // circular list where last_paint.next points to the first paint
    let next = if let Some(last) = node.last_paint.index() {
        let first = kernel.paints[last].next;
        kernel.paints[last].next = id;
        first
    } else {
        id
    };
    kernel.paints.push(StoredPaint {
        run: run::<C, F>,
        bump_id,
        next,
    });
    node.last_paint = id;
}

pub fn resolve_order<C: Context>(kernel: &mut Kernel<C>, bump: &BumpArena) {
    kernel.paint_order.clear();
    let count = kernel.nodes.len();
    let placements = &kernel.placements[..count];
    if !kernel
        .nodes
        .iter()
        .zip(placements)
        .any(|(node, placement)| placement.z_index != 0 || visual_parent(&kernel.aux, node) != node.parent)
    {
        return;
    }

    let mut scratch = bump.storage.slice(count);
    scratch.resize(count, PaintLinks::default());
    let links = &mut scratch[..count];
    // zero marks the end of a child list since the root has no parent
    for index in 1..count {
        let parent = visual_parent(&kernel.aux, &kernel.nodes[index]).index();
        links[index].next_sibling = links[parent].first_child;
        links[parent].first_child = index as u32;
    }

    // completed nodes fill the front while pending nodes form a stack at the back
    kernel.paint_order.resize(count, NodeId::new(0));
    let order = &mut kernel.paint_order[..count];
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

pub fn resolve_clips<C: Context>(kernel: &mut Kernel<C>) {
    kernel.interaction.clips.clear();
    let count = kernel.nodes.len();
    let nodes = &kernel.nodes[..count];
    let placements = &kernel.placements[..count];
    let aux = &kernel.aux;
    kernel.resolved_clips.clear();
    kernel.resolved_clips.resize(count, ResolvedClipId::NONE);
    let resolved = &mut kernel.resolved_clips[..count];
    for index in 0..count {
        let parent = if index == 0 {
            ResolvedClipId::NONE
        } else {
            resolved[visual_parent(aux, &nodes[index]).index()]
        };
        resolved[index] = parent;
        let Some(radius) = nodes[index].aux.index().and_then(|index| aux[index].clip) else {
            continue;
        };
        let area = placements[index].area;
        let bounds = parent
            .index()
            .map_or(kernel.screen, |clip| kernel.interaction.clips[clip].bounds)
            .intersection(area)
            .unwrap_or_default();
        let id = ResolvedClipId::new(kernel.interaction.clips.len());
        let depth = parent
            .index()
            .map_or(1, |parent| kernel.interaction.clips[parent].depth + 1);
        kernel.interaction.clips.push(ResolvedClip {
            parent,
            depth,
            area,
            radius,
            bounds,
        });
        resolved[index] = id;
    }
}

pub fn paint<C: Context>(kernel: &Kernel<C>, bump: &BumpArena, context: &mut C) {
    // the clips applied to the context, at most every clip deep
    let mut active = bump.storage.slice(kernel.interaction.clips.len());
    if kernel.paint_order.is_empty() {
        for node in 0..kernel.nodes.len() {
            paint_node(kernel, bump, context, &mut active, node);
        }
    } else {
        for index in 0..kernel.paint_order.len() {
            let node = kernel.paint_order[index].index();
            paint_node(kernel, bump, context, &mut active, node);
        }
    }
    set(kernel, context, &mut active, ResolvedClipId::NONE);
}

fn set<C: Context>(
    kernel: &Kernel<C>,
    context: &mut C,
    active: &mut Scratch<'_, ResolvedClipId>,
    target: ResolvedClipId,
) {
    let mut common = target;
    while let Some(index) = common.index() {
        let depth = kernel.interaction.clips[index].depth as usize;
        if depth <= active.len() && active[depth - 1].0 == common.0 {
            break;
        }
        common = kernel.interaction.clips[index].parent;
    }
    while active.last().map_or(u32::MAX, |clip| clip.0) != common.0 {
        active.pop();
        context.pop_clip();
    }
    push(kernel, context, active, target, common.0);

    fn push<C: Context>(
        kernel: &Kernel<C>,
        context: &mut C,
        active: &mut Scratch<'_, ResolvedClipId>,
        clip: ResolvedClipId,
        common: u32,
    ) {
        if clip.0 == common {
            return;
        }
        let stored = kernel.interaction.clips[clip.index().unwrap()];
        push(kernel, context, active, stored.parent, common);
        context.push_clip(stored.area, stored.radius);
        active.push(clip);
    }
}

fn paint_node<C: Context>(
    kernel: &Kernel<C>,
    bump: &BumpArena,
    context: &mut C,
    active: &mut Scratch<'_, ResolvedClipId>,
    node: usize,
) {
    let Some(last) = kernel.nodes[node].last_paint.index() else {
        return;
    };
    let area = kernel.placements[node].area;
    let resolved_clip = kernel.resolved_clips[node];
    // painters may draw past their area so renderers cull by what they draw
    let bounds = resolved_clip
        .index()
        .map_or(kernel.screen, |clip| kernel.interaction.clips[clip].bounds);
    if bounds.width <= C::Scalar::ZERO || bounds.height <= C::Scalar::ZERO {
        return;
    }
    set(kernel, context, active, resolved_clip);
    let mut index = kernel.paints[last].next.0 as usize;
    loop {
        let stored = kernel.paints[index];
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
