use super::{Frame, NodeId, ResolvedClip, ResolvedClipId};
use crate::{Context, arena::DataArena};

pub fn resolve_order<C: Context>(frame: &mut Frame<C>) {
    frame.paint_order.clear();
    if !frame
        .nodes
        .iter()
        .enumerate()
        .any(|(index, node)| frame.node_geometry[index].z_index != 0 || node.visual_parent != node.parent)
    {
        return;
    }

    frame.paint_links.clear();
    frame
        .paint_links
        .resize(frame.nodes.len(), super::PaintLinks::default());
    // zero marks the end of a child list since the root has no parent
    for index in 1..frame.nodes.len() {
        let parent = frame.nodes[index].visual_parent.index();
        frame.paint_links[index].next_sibling = frame.paint_links[parent].first_child;
        frame.paint_links[parent].first_child = index as u32;
    }

    frame.order_stack.clear();
    frame.order_stack.push(NodeId::new(0));
    while let Some(parent) = frame.order_stack.pop() {
        frame.paint_order.push(parent);
        let start = frame.order_stack.len();
        let mut child = frame.paint_links[parent.index()].first_child;
        while child != 0 {
            frame.order_stack.push(NodeId::new(child as usize));
            child = frame.paint_links[child as usize].next_sibling;
        }
        let children = &mut frame.order_stack[start..];
        if children.iter().any(|id| frame.node_geometry[id.index()].z_index != 0) {
            children
                .sort_unstable_by_key(|id| std::cmp::Reverse((frame.node_geometry[id.index()].z_index, id.index())));
        }
    }
    debug_assert_eq!(frame.paint_order.len(), frame.nodes.len());
}

pub fn resolve_clips<C: Context>(frame: &mut Frame<C>) {
    frame.resolved_clips.clear();
    for index in 0..frame.nodes.len() {
        let parent = if index == 0 {
            ResolvedClipId::NONE
        } else {
            frame.nodes[frame.nodes[index].visual_parent.index()].resolved_clip
        };
        if frame.nodes[index].clip.index().is_some() {
            let bounds = frame
                .clip_bounds(parent)
                .intersection(frame.node_geometry[index].area)
                .unwrap_or_default();
            let id = ResolvedClipId::new(frame.resolved_clips.len());
            let depth = parent
                .index()
                .map_or(1, |parent| frame.resolved_clips[parent].depth + 1);
            frame.resolved_clips.push(ResolvedClip {
                parent,
                depth,
                clip: frame.nodes[index].clip,
                area: frame.node_geometry[index].area,
                bounds,
            });
            frame.nodes[index].resolved_clip = id;
        } else {
            frame.nodes[index].resolved_clip = parent;
        }
    }
}

pub fn render<C: Context>(frame: &mut Frame<C>, data: &DataArena, context: &mut C) {
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
        let depth = frame.resolved_clips[index].depth as usize;
        if depth <= frame.active_clips.len() && frame.active_clips[depth - 1].0 == common.0 {
            break;
        }
        common = frame.resolved_clips[index].parent;
    }
    while frame.active_clips.last().map_or(u32::MAX, |clip| clip.0) != common.0 {
        let clip = frame.active_clips.pop().unwrap();
        let stored = frame.resolved_clips[clip.index().unwrap()];
        let clip_data = frame.clips[stored.clip.index().unwrap()];
        (frame.clip_kinds[clip_data.kind as usize].pop)(data, clip_data.data, context);
    }
    push(frame, data, context, target, common.0);

    fn push<C: Context>(frame: &mut Frame<C>, data: &DataArena, context: &mut C, clip: ResolvedClipId, common: u32) {
        if clip.0 == common {
            return;
        }
        let stored = frame.resolved_clips[clip.index().unwrap()];
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
