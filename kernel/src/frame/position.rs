use super::{Frame, NodeId, Sizing};
use crate::{
    Platform,
    arena::DataArena,
    geometry::{Constraints, Point, Size},
    layout::Axis,
};

pub fn layout<R: Platform>(frame: &mut Frame<R>, data: &DataArena, platform: &mut R, size: Size) {
    let root = frame.node_id(0);
    frame.layout_node(data, root, platform, Constraints::tight(size));
    for index in 1..frame.nodes.len() {
        let Some(positioned) = frame.nodes[index].positioned.index() else {
            continue;
        };
        let positioned = frame.positioned[positioned];
        let node = frame.node_id(index);
        let target = frame.nodes[positioned.target.index()].area;
        let containing = frame.nodes[index]
            .layer
            .map_or(frame.nodes[index].parent, |layer| {
                frame.layers[layer.index()].owner
            });
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
            Sizing::Percent(_) => {
                let size = sizing.resolve(0.0, available, true);
                (size, size)
            }
        };
        let transition = if frame.resolving_size_transition {
            frame.nodes[index]
                .geometry
                .index()
                .map(|index| frame.geometry[index])
        } else {
            None
        };
        let absolute_sizing = *data.load::<super::AbsoluteSizing>(frame.nodes[index].item);
        let res = frame.layout_resolution;
        let sizing = |axis, sizing: Sizing, property, size| {
            transition
                .filter(|geometry| geometry.transition_properties.intersects(property))
                .map(|_| res.sizing(axis, Sizing::fixed(size)))
                .unwrap_or(sizing)
        };
        let width = range(
            sizing(
                Axis::Horizontal,
                absolute_sizing.width,
                crate::TransitionProperties::WIDTH,
                transition.map_or(0.0, |geometry| geometry.transition_size.width),
            ),
            available.width,
        );
        let height = range(
            sizing(
                Axis::Vertical,
                absolute_sizing.height,
                crate::TransitionProperties::HEIGHT,
                transition.map_or(0.0, |geometry| geometry.transition_size.height),
            ),
            available.height,
        );
        let size = frame.layout_node(
            data,
            node,
            platform,
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

pub fn offset<R: Platform>(frame: &Frame<R>, node: NodeId) -> Point {
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

pub fn resolve<R: Platform>(frame: &mut Frame<R>) {
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
