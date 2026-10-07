pub mod performance;
blit_widgets::export!(i32, crate::TuiContext, crate::BoundsClip);

pub mod block;
pub mod text;
pub mod text_input;

pub use self::{
    block::{Block, Title},
    performance::Performance,
    text::Text,
    text_input::TextInput,
};
