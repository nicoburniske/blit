use blit_gui::{
    GuiContext,
    image::{ImageData, ImageHandle},
};
use blit_widgets::performance;

use crate::{GraphicsBackend, GraphicsError};

pub struct DesktopContext {
    pub gui: GuiContext,
    pub performance: performance::State,
    pub graphics: Box<dyn GraphicsBackend>,
}

impl DesktopContext {
    pub fn new(gui: GuiContext, graphics: Box<dyn GraphicsBackend>) -> Self {
        Self {
            gui,
            performance: performance::State::default(),
            graphics,
        }
    }

    pub fn create_image(&mut self, data: ImageData) -> Result<ImageHandle, GraphicsError> {
        self.graphics.create_image(data)
    }
}

impl blit::Context for DesktopContext {
    type Scalar = f32;
}

impl AsMut<GuiContext> for DesktopContext {
    fn as_mut(&mut self) -> &mut GuiContext {
        &mut self.gui
    }
}

impl AsRef<performance::State> for DesktopContext {
    fn as_ref(&self) -> &performance::State {
        &self.performance
    }
}
