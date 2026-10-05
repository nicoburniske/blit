use super::Frame;

pub fn resolve<C>(frame: &mut Frame<C>) {
    for index in 1..frame.nodes.len() {
        let reference = frame.nodes[index].relative;
        let reference = frame.node_geometry[reference.index()].area;
        let area = &mut frame.node_geometry[index].area;
        area.x += reference.x;
        area.y += reference.y;
    }
}
