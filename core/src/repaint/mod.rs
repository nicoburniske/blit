mod full;
mod incremental;

pub use self::{full::*, incremental::*};

use crate::{command_list::CommandList, geometry::PhysicalRect, renderer::Renderer};

/// controls how resolved command lists are rendered and retained
pub trait Repaint {
    /// makes the next frame repaint the full screen
    fn invalidate(&mut self);

    fn render<R: Renderer>(
        &mut self,
        renderer: &mut R,
        commands: &mut CommandList,
        screen: PhysicalRect,
    );
}
