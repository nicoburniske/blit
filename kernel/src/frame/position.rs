use super::{Frame, NodeId};
use crate::{
    geometry::{Constraints, Point, Size},
    renderer::Renderer,
};

pub fn layout<R: Renderer>(frame: &mut Frame<R>, renderer: &mut R, size: Size) {
    frame.layout_node(NodeId(0), renderer, Constraints::tight(size));
    for index in 1..frame.nodes.len() {
        let Some(positioned) = frame.nodes[index].positioned else {
            continue;
        };
        let node = NodeId(index as u32);
        let target = frame.nodes[positioned.target.index()].area;
        let size = frame.layout_node(node, renderer, Constraints::loose(target.size()));
        let target_anchor = anchor(positioned.target_anchor);
        let child_anchor = anchor(positioned.child_anchor);
        frame.nodes[index].area.x =
            target.width * target_anchor.x - size.width * child_anchor.x + positioned.offset.x;
        frame.nodes[index].area.y =
            target.height * target_anchor.y - size.height * child_anchor.y + positioned.offset.y;
    }
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
