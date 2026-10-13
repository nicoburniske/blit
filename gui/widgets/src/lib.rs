pub mod performance;
pub mod text_input;

pub use blit_gui::widget::{RichText, Text};
pub use blit_widgets::scroll;

pub use self::{performance::Performance, text_input::TextInput};

pub mod layout {
    blit_layout::export!(f32);
}

blit_widgets::export!(f32);
