use super::Frame;
use crate::Context;

pub fn resolve<C: Context>(frame: &mut Frame<C>) {
    let count = frame.nodes.len();
    let geometry = &mut frame.node_geometry[..count];
    for index in 1..count {
        let reference = frame.nodes[index].relative;
        let reference = geometry[reference.index()].area;
        let area = &mut geometry[index].area;
        area.x += reference.x;
        area.y += reference.y;
    }
}
