pub mod block;
pub mod performance;
pub mod scroll_area;
pub mod scroll_list;
pub mod text;
pub mod text_input;
pub mod virtual_list;

pub mod popover {
    pub use blit_widgets::popover::{Close, State, new};
    pub type Config = blit_widgets::popover::Config<u16>;
}

pub mod resize {
    pub use blit_widgets::resize::{Edge, Grip, State, new};
    pub type Config = blit_widgets::resize::Config<u16>;
}

pub mod split {
    pub use blit_widgets::split::{State, new};
    pub type Config = blit_widgets::split::Config<u16>;
}

pub use self::{
    block::{Block, Title},
    performance::Performance,
    text::Text,
    text_input::TextInput,
};
