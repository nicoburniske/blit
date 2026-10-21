use super::{Frame, NodeId, ResolvedClip, ResolvedClipId};
use crate::{ClipShape, Context, Rect, Scalar, arena::DataArena};

pub fn resolve_order<C: Context>(frame: &mut Frame<C>, data: &DataArena) {
    frame.paint_order.clear();
    let count = frame.nodes.len();
    if !frame
        .nodes
        .iter()
        .enumerate()
        .any(|(index, node)| frame.node_geometry[index].z_index != 0 || node.visual_parent != node.parent)
    {
        return;
    }

    let mut scratch = data.scratch(count, PaintLinks::default());
    let links = &mut *scratch;
    // zero marks the end of a child list since the root has no parent
    for index in 1..count {
        let parent = frame.nodes[index].visual_parent.index();
        links[index].next_sibling = links[parent].first_child;
        links[parent].first_child = index as u32;
    }

    // completed nodes fill the front while pending nodes form a stack at the back
    frame.paint_order.resize(count, NodeId::new(0));
    let order = &mut frame.paint_order;
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
            needs_sort = needs_sort || frame.node_geometry[index].z_index != 0;
            pending -= 1;
            debug_assert!(pending > output);
            order[pending] = NodeId::new(index);
        }
        let children = &mut order[pending..start];
        if needs_sort {
            children.sort_unstable_by_key(|id| (frame.node_geometry[id.index()].z_index, id.index()));
        }
    }
    debug_assert_eq!(pending, count);
}

pub fn resolve_clips<C: Context>(frame: &mut Frame<C>, data: &DataArena) {
    frame.interaction.clips.clear();
    for index in 0..frame.nodes.len() {
        let parent = if index == 0 {
            ResolvedClipId::NONE
        } else {
            frame.nodes[frame.nodes[index].visual_parent.index()].resolved_clip
        };
        frame.nodes[index].resolved_clip = parent;
        let Some(clip) = frame.nodes[index].clip.index() else {
            continue;
        };
        let stored = frame.clips[clip];
        let area = frame.node_geometry[index].area;
        let shape = (frame.clip_kinds[stored.kind as usize].shape)(data, stored.data, area);
        let parent_bounds = frame.clip_bounds(parent);
        let bounds = match shape {
            ClipShape::Unbounded => continue,
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
            clip: frame.nodes[index].clip,
            area,
            bounds,
            shape,
        });
        frame.nodes[index].resolved_clip = id;
    }
}

pub fn paint<C: Context>(frame: &mut Frame<C>, data: &DataArena, context: &mut C) {
    frame.active_clips.clear();
    if frame.paint_order.is_empty() {
        for node in 0..frame.nodes.len() {
            paint_node(frame, data, context, node);
        }
    } else {
        for index in 0..frame.paint_order.len() {
            let node = frame.paint_order[index].index();
            paint_node(frame, data, context, node);
        }
    }
    set(frame, data, context, ResolvedClipId::NONE);
}

fn set<C: Context>(frame: &mut Frame<C>, data: &DataArena, context: &mut C, target: ResolvedClipId) {
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
        let clip_data = frame.clips[stored.clip.index().unwrap()];
        (frame.clip_kinds[clip_data.kind as usize].pop)(data, clip_data.data, context);
    }
    push(frame, data, context, target, common.0);

    fn push<C: Context>(frame: &mut Frame<C>, data: &DataArena, context: &mut C, clip: ResolvedClipId, common: u32) {
        if clip.0 == common {
            return;
        }
        let stored = frame.interaction.clips[clip.index().unwrap()];
        push(frame, data, context, stored.parent, common);
        let clip_data = frame.clips[stored.clip.index().unwrap()];
        (frame.clip_kinds[clip_data.kind as usize].push)(data, clip_data.data, context, stored.area);
        frame.active_clips.push(clip);
    }
}

fn paint_node<C: Context>(frame: &mut Frame<C>, data: &DataArena, context: &mut C, node: usize) {
    if frame.nodes[node].first_atom.index().is_none() {
        return;
    }
    let area = frame.node_geometry[node].area;
    let resolved_clip = frame.nodes[node].resolved_clip;
    let clip_bounds = frame.clip_bounds(resolved_clip);
    let mut clip_set = false;
    let mut atom = frame.nodes[node].first_atom;
    while let Some(atom_index) = atom.index() {
        let stored = frame.atoms[atom_index];
        let kind = &frame.atom_kinds[stored.kind as usize];
        if (kind.paint_bounds)(data, stored.data, area)
            .intersection(clip_bounds)
            .is_none()
        {
            atom = stored.next;
            continue;
        }
        let paint = kind.paint;
        if !clip_set {
            set(frame, data, context, resolved_clip);
            clip_set = true;
        }
        paint(data, stored.data, context, area);
        atom = stored.next;
    }
}

#[derive(Clone, Copy, Default)]
struct PaintLinks {
    first_child: u32,
    next_sibling: u32,
}
