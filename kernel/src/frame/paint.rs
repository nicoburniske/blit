use super::{ClipKind, Frame, ResolvedClip, ResolvedClipId, StoredClip};
use crate::{FrameInfo, Platform, arena::DataArena};

pub fn resolve_order<R: Platform>(frame: &mut Frame<R>) {
    frame.paint_order.clear();
    if !frame
        .nodes
        .iter()
        .any(|node| node.z_index != 0 || node.visual_parent != node.parent)
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
    frame.order_stack.push(frame.node_id(0));
    while let Some(parent) = frame.order_stack.pop() {
        frame.paint_order.push(parent);
        let start = frame.order_stack.len();
        let mut child = frame.paint_links[parent.index()].first_child;
        while child != 0 {
            frame.order_stack.push(frame.node_id(child as usize));
            child = frame.paint_links[child as usize].next_sibling;
        }
        let children = &mut frame.order_stack[start..];
        if children
            .iter()
            .any(|id| frame.nodes[id.index()].z_index != 0)
        {
            children.sort_unstable_by_key(|id| {
                std::cmp::Reverse((frame.nodes[id.index()].z_index, id.index()))
            });
        }
    }
    debug_assert_eq!(frame.paint_order.len(), frame.nodes.len());
}

pub fn resolve_clips<R: Platform>(frame: &mut Frame<R>) {
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
                .intersection(frame.nodes[index].area)
                .unwrap_or_default();
            let id = ResolvedClipId::new(frame.resolved_clips.len());
            let depth = parent
                .index()
                .map_or(1, |parent| frame.resolved_clips[parent].depth + 1);
            frame.resolved_clips.push(ResolvedClip {
                parent,
                depth,
                clip: frame.nodes[index].clip,
                area: frame.nodes[index].area,
                bounds,
            });
            frame.nodes[index].resolved_clip = id;
        } else {
            frame.nodes[index].resolved_clip = parent;
        }
    }
}

pub fn render<R: Platform>(
    frame: &mut Frame<R>,
    data: &DataArena,
    platform: &mut R,
    info: FrameInfo,
) {
    platform.begin(info);
    frame.active_clips.clear();
    if frame.paint_order.is_empty() {
        for node in 0..frame.nodes.len() {
            paint_node(frame, data, platform, node);
        }
    } else {
        for index in 0..frame.paint_order.len() {
            let node = frame.paint_order[index].index();
            paint_node(frame, data, platform, node);
        }
    }
    set(
        data,
        &frame.clips,
        &frame.clip_kinds,
        &frame.resolved_clips,
        &mut frame.active_clips,
        platform,
        ResolvedClipId::NONE,
    );
    platform.end();
}

#[allow(clippy::too_many_arguments)]
fn push<R: Platform>(
    data: &DataArena,
    clips: &[StoredClip],
    kinds: &[ClipKind<R>],
    resolved: &[ResolvedClip],
    active: &mut Vec<ResolvedClipId>,
    platform: &mut R,
    clip: ResolvedClipId,
    common: u32,
) {
    if clip.0 == common {
        return;
    }
    let stored = resolved[clip.index().unwrap()];
    push(
        data,
        clips,
        kinds,
        resolved,
        active,
        platform,
        stored.parent,
        common,
    );
    let clip_data = clips[stored.clip.index().unwrap()];
    (kinds[clip_data.kind as usize].push)(data, clip_data.data, platform, stored.area);
    active.push(clip);
}

fn set<R: Platform>(
    data: &DataArena,
    clips: &[StoredClip],
    kinds: &[ClipKind<R>],
    resolved: &[ResolvedClip],
    active: &mut Vec<ResolvedClipId>,
    platform: &mut R,
    target: ResolvedClipId,
) {
    let mut common = target;
    while let Some(index) = common.index() {
        let depth = resolved[index].depth as usize;
        if depth <= active.len() && active[depth - 1].0 == common.0 {
            break;
        }
        common = resolved[index].parent;
    }
    while active.last().map_or(u32::MAX, |clip| clip.0) != common.0 {
        let clip = active.pop().unwrap();
        let stored = resolved[clip.index().unwrap()];
        let clip_data = clips[stored.clip.index().unwrap()];
        (kinds[clip_data.kind as usize].pop)(data, clip_data.data, platform);
    }
    push(
        data, clips, kinds, resolved, active, platform, target, common.0,
    );
}

fn paint_node<R: Platform>(frame: &mut Frame<R>, data: &DataArena, platform: &mut R, node: usize) {
    if frame.nodes[node].first_atom.index().is_none() {
        return;
    }
    let area = frame.nodes[node].area;
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
        if !clip_set {
            set(
                data,
                &frame.clips,
                &frame.clip_kinds,
                &frame.resolved_clips,
                &mut frame.active_clips,
                platform,
                resolved_clip,
            );
            clip_set = true;
        }
        (kind.paint)(data, stored.data, platform, area);
        atom = stored.next;
    }
}
