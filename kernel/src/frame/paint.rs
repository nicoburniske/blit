use super::{ClipKind, Frame, ResolvedClip, ResolvedClipId, StoredClip, container};
use crate::{
    arena::DataArena,
    platform::{FrameInfo, Platform},
};

pub fn resolve_order<R: Platform>(frame: &mut Frame<R>) {
    frame.paint_order.clear();
    if !frame.needs_paint_order {
        return;
    }

    frame.order_stack.clear();
    frame.order_stack.push(frame.node_id(0));
    while let Some(parent) = frame.order_stack.pop() {
        frame.paint_order.push(parent);
        let start = frame.order_stack.len();
        let mut child = parent.index() + 1;
        let end = frame.nodes[parent.index()].subtree_end as usize;
        while child <= end {
            if frame.nodes[child].slot.layer.is_none() {
                frame.order_stack.push(frame.node_id(child));
            }
            child = frame.nodes[child].subtree_end as usize + 1;
        }
        if parent.index() == 0 && !frame.layers.is_empty() {
            for index in 1..frame.nodes.len() {
                if frame.nodes[index].slot.layer.is_some() {
                    frame.order_stack.push(frame.node_id(index));
                }
            }
        }
        if frame.order_stack.len() - start <= 1 {
            continue;
        }
        let children = &mut frame.order_stack[start..];
        if children.iter().any(|node| {
            let node = &frame.nodes[node.index()];
            node.slot.layer.is_some() || node.slot.z_index != 0
        }) {
            children.sort_unstable_by(|a, b| {
                let a = a.index();
                let b = b.index();
                let a_layer = frame.nodes[a].slot.layer.map_or(0, container::layer_order);
                let b_layer = frame.nodes[b].slot.layer.map_or(0, container::layer_order);
                (b_layer, frame.nodes[b].slot.z_index, b).cmp(&(
                    a_layer,
                    frame.nodes[a].slot.z_index,
                    a,
                ))
            });
        } else {
            children.reverse();
        }
    }
    debug_assert_eq!(frame.paint_order.len(), frame.nodes.len());
}

pub fn resolve_clips<R: Platform>(frame: &mut Frame<R>) {
    frame.resolved_clips.clear();
    for index in 0..frame.nodes.len() {
        let parent = if index == 0 {
            ResolvedClipId::NONE
        } else if let Some(layer) = frame.nodes[index].slot.layer {
            let owner = frame.layers[container::layer_index(layer)].owner.index();
            frame.nodes[owner].resolved_clip
        } else {
            frame.nodes[frame.nodes[index].parent.index()].resolved_clip
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

pub fn render<R: Platform>(frame: &mut Frame<R>, platform: &mut R, info: FrameInfo) {
    // todo: skip leaves outside their conservative paint bounds
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

    platform.begin(info);
    frame.active_clips.clear();
    if frame.paint_order.is_empty() {
        for index in 0..frame.nodes.len() {
            let Some(base) = frame.nodes[index].base.index() else {
                continue;
            };
            set(
                &frame.data,
                &frame.clips,
                &frame.clip_kinds,
                &frame.resolved_clips,
                &mut frame.active_clips,
                platform,
                frame.nodes[index].resolved_clip,
            );
            let base = frame.leaves[base];
            (frame.leaf_kinds[base.kind as usize].paint)(
                &frame.data,
                base.data,
                platform,
                frame.nodes[index].area,
            );
        }
    } else {
        for index in 0..frame.paint_order.len() {
            let node = frame.paint_order[index].index();
            let Some(base) = frame.nodes[node].base.index() else {
                continue;
            };
            set(
                &frame.data,
                &frame.clips,
                &frame.clip_kinds,
                &frame.resolved_clips,
                &mut frame.active_clips,
                platform,
                frame.nodes[node].resolved_clip,
            );
            let base = frame.leaves[base];
            (frame.leaf_kinds[base.kind as usize].paint)(
                &frame.data,
                base.data,
                platform,
                frame.nodes[node].area,
            );
        }
    }
    set(
        &frame.data,
        &frame.clips,
        &frame.clip_kinds,
        &frame.resolved_clips,
        &mut frame.active_clips,
        platform,
        ResolvedClipId::NONE,
    );
    platform.end();
}
