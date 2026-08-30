use super::{Frame, NodeId, Sizing};
use crate::{
    geometry::{Constraints, Point, Size},
    renderer::Renderer,
};

pub fn layout<R: Renderer>(frame: &mut Frame<R>, renderer: &mut R, size: Size) {
    let root = frame.node_id(0);
    frame.layout_node(root, renderer, Constraints::tight(size));
    for index in 1..frame.nodes.len() {
        let Some(positioned) = frame.nodes[index].positioned else {
            continue;
        };
        let node = frame.node_id(index);
        let target = frame.nodes[positioned.target.index()].area;
        let range = |sizing: Sizing, available: f32| match sizing {
            Sizing::Fit { min, max } => {
                let min = min.max(0.0);
                (min, max.max(min).min(available).max(min))
            }
            Sizing::Grow { .. } => {
                let size = sizing.clamp(available);
                (size, size)
            }
            Sizing::Fixed(size) => {
                let size = size.max(0.0);
                (size, size)
            }
            Sizing::Percent(_) => {
                let size = sizing.resolve(0.0, available, true);
                (size, size)
            }
        };
        let width = range(frame.nodes[index].slot.width, target.width);
        let height = range(frame.nodes[index].slot.height, target.height);
        let size = frame.layout_node(
            node,
            renderer,
            Constraints {
                min: Size::new(width.0, height.0),
                max: Size::new(width.1, height.1),
            },
        );
        let target_anchor = anchor(positioned.target_anchor);
        let child_anchor = anchor(positioned.child_anchor);
        let reference_offset = offset(frame, node);
        frame.nodes[index].area.x = target.width * target_anchor.x - size.width * child_anchor.x
            + positioned.offset.x
            + reference_offset.x;
        frame.nodes[index].area.y = target.height * target_anchor.y - size.height * child_anchor.y
            + positioned.offset.y
            + reference_offset.y;
    }
}

pub fn offset<R: Renderer>(frame: &Frame<R>, node: NodeId) -> Point {
    if let Some(positioned) = frame.nodes[node.index()].positioned {
        return if positioned.uses_target_content_origin {
            frame.nodes[positioned.target.index()].content_offset
        } else {
            Point::ZERO
        };
    }
    frame.nodes[node.index()]
        .parent
        .map_or(Point::ZERO, |parent| {
            frame.nodes[parent.index()].content_offset
        })
}

pub fn resolve<R: Renderer>(frame: &mut Frame<R>) {
    for index in 1..frame.nodes.len() {
        let reference = frame.nodes[index].positioned.map_or_else(
            || frame.nodes[index].parent.unwrap(),
            |positioned| positioned.target,
        );
        frame.nodes[index].area.x += frame.nodes[reference.index()].area.x;
        frame.nodes[index].area.y += frame.nodes[reference.index()].area.y;
    }
}

fn anchor(anchor: super::Anchor) -> Point {
    match anchor {
        super::Anchor::TopLeft => Point::new(0.0, 0.0),
        super::Anchor::Top => Point::new(0.5, 0.0),
        super::Anchor::TopRight => Point::new(1.0, 0.0),
        super::Anchor::Left => Point::new(0.0, 0.5),
        super::Anchor::Center => Point::new(0.5, 0.5),
        super::Anchor::Right => Point::new(1.0, 0.5),
        super::Anchor::BottomLeft => Point::new(0.0, 1.0),
        super::Anchor::Bottom => Point::new(0.5, 1.0),
        super::Anchor::BottomRight => Point::new(1.0, 1.0),
    }
}
