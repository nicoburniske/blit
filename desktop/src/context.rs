use blit_gui::GuiContext;
use blit_widgets::performance;

pub struct DesktopContext {
    pub gui: GuiContext,
    pub performance: performance::State,
}

impl DesktopContext {
    pub fn new(gui: GuiContext) -> Self {
        Self {
            gui,
            performance: performance::State::default(),
        }
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
