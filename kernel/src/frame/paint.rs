use super::{Frame, NodeId, container};
use crate::{
    geometry::Rect,
    renderer::{FrameInfo, Renderer},
};

pub fn resolve_order<R: Renderer>(frame: &mut Frame<R>) {
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
            let node = frame.nodes[node.index()];
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

pub fn resolve_clips<R: Renderer>(frame: &mut Frame<R>, screen: Rect) {
    frame.resolved_clips.clear();
    for index in 0..frame.nodes.len() {
        let (parent, bounds) = if index == 0 {
            (None, screen)
        } else if let Some(layer) = frame.nodes[index].slot.layer {
            let owner = frame.layers[container::layer_index(layer)].owner.index();
            (
                frame.nodes[owner].resolved_clip,
                frame.nodes[owner].clip_bounds,
            )
        } else {
            let parent = frame.nodes[index].parent.unwrap().index();
            (
                frame.nodes[parent].resolved_clip,
                frame.nodes[parent].clip_bounds,
            )
        };
        if let Some(clip) = frame.nodes[index].clip {
            let bounds = bounds
                .intersection(frame.nodes[index].area)
                .unwrap_or_default();
            let id = frame.resolved_clips.len();
            frame.resolved_clips.push(super::ResolvedClip {
                parent,
                clip,
                area: frame.nodes[index].area,
            });
            frame.nodes[index].resolved_clip = Some(id);
            frame.nodes[index].clip_bounds = bounds;
        } else {
            frame.nodes[index].resolved_clip = parent;
            frame.nodes[index].clip_bounds = bounds;
        }
    }
}

pub fn render<R: Renderer>(frame: &Frame<R>, renderer: &mut R, info: FrameInfo) {
    fn push<R: Renderer>(frame: &Frame<R>, renderer: &mut R, clip: usize) {
        let resolved = frame.resolved_clips[clip];
        if let Some(parent) = resolved.parent {
            push(frame, renderer, parent);
        }
        let call = frame.clip_kinds[resolved.clip.kind as usize].push;
        call(&frame.data, resolved.clip.data, renderer, resolved.area);
    }

    fn paint<R: Renderer>(frame: &Frame<R>, renderer: &mut R, node: NodeId) {
        let stored = frame.nodes[node.index()];
        let Some(base) = stored.base else {
            return;
        };
        if let Some(clip) = stored.resolved_clip {
            push(frame, renderer, clip);
        }
        let call = frame.leaf_kinds[base.kind as usize].paint;
        call(&frame.data, base.data, renderer, stored.area);
        let mut clip = stored.resolved_clip;
        while let Some(id) = clip {
            let resolved = frame.resolved_clips[id];
            let call = frame.clip_kinds[resolved.clip.kind as usize].pop;
            call(&frame.data, resolved.clip.data, renderer);
            clip = resolved.parent;
        }
    }

    renderer.begin(info);
    if frame.paint_order.is_empty() {
        for index in 0..frame.nodes.len() {
            paint(frame, renderer, frame.node_id(index));
        }
    } else {
        for node in frame.paint_order.iter().copied() {
            paint(frame, renderer, node);
        }
    }
    renderer.end();
}
