use crate::{command_list::CommandList, geometry::PhysicalRect, renderer::Renderer};

use super::Repaint;

/// repaints the full screen without retaining frame history
#[derive(Clone, Copy, Debug, Default)]
pub struct FullRepaint;

impl Repaint for FullRepaint {
    fn invalidate(&mut self) {}

    fn render<R: Renderer>(
        &mut self,
        renderer: &mut R,
        commands: &mut CommandList,
        screen: PhysicalRect,
    ) {
        renderer.render(commands, &[screen]);
    }
}
