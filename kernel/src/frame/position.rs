use super::Frame;
use crate::Context;

pub fn resolve<C: Context>(frame: &mut Frame<C>) {
    let count = frame.nodes.len();
    let placements = &mut frame.placements[..count];
    for index in 1..count {
        let reference = frame.nodes[index].relative;
        let reference = placements[reference.index()].area;
        let area = &mut placements[index].area;
        area.x += reference.x;
        area.y += reference.y;
    }
}
