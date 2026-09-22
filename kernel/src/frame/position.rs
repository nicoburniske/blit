use super::{Frame, NodeId};
use crate::{
    arena::DataArena,
    geometry::{Constraints, Point, Size},
    layout::{Axis, Sizing},
};

pub fn layout<C>(frame: &mut Frame<C>, data: &DataArena, context: &mut C, size: Size) {
    let root = frame.node_id(0);
    frame.layout_node(data, root, context, Constraints::tight(size));
    for index in 1..frame.nodes.len() {
        let Some(positioned) = frame.nodes[index].positioned.index() else {
            continue;
        };
        let positioned = frame.positioned[positioned];
        let node = frame.node_id(index);
        let target = frame.nodes[positioned.target.index()].area;
        let containing = frame.nodes[index].visual_parent;
        let available = frame.nodes[containing.index()].area.size();
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
            Sizing::Percent(fraction) if available.is_finite() => {
                assert!((0.0..=1.0).contains(&fraction));
                let size = available * fraction;
                (size, size)
            }
            Sizing::Percent(_) => (0.0, 0.0),
        };
        let transition = if !frame.target_sizes.is_empty() {
            Some((frame.nodes[index].area.size(), frame.target_sizes[index]))
        } else {
            None
        };
        let absolute_sizing = *data.load::<super::AbsoluteSizing>(frame.nodes[index].item);
        let res = frame.layout_resolution;
        let sizing = |axis: Axis, sizing: Sizing, property, size| {
            transition
                .filter(|(_, target)| target.properties.intersects(property))
                .map(|_| res.sizing(axis, Sizing::fixed(size)))
                .unwrap_or(sizing)
        };
        let width = range(
            sizing(
                Axis::Horizontal,
                absolute_sizing.width,
                crate::TransitionProperties::WIDTH,
                transition.map_or(0.0, |(current, _)| current.width),
            ),
            available.width,
        );
        let height = range(
            sizing(
                Axis::Vertical,
                absolute_sizing.height,
                crate::TransitionProperties::HEIGHT,
                transition.map_or(0.0, |(current, _)| current.height),
            ),
            available.height,
        );
        let size = frame.layout_node(
            data,
            node,
            context,
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

pub fn offset<C>(frame: &Frame<C>, node: NodeId) -> Point {
    if let Some(positioned) = frame.nodes[node.index()].positioned.index() {
        let positioned = frame.positioned[positioned];
        return if positioned.uses_target_content_origin {
            frame.layout_offset(positioned.target)
        } else {
            Point::ZERO
        };
    }
    let parent = frame.nodes[node.index()].parent;
    if parent == node {
        Point::ZERO
    } else {
        frame.layout_offset(parent)
    }
}

pub fn resolve<C>(frame: &mut Frame<C>) {
    for index in 1..frame.nodes.len() {
        let reference = frame.nodes[index]
            .positioned
            .index()
            .map_or(frame.nodes[index].parent, |positioned| {
                frame.positioned[positioned].target
            });
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
