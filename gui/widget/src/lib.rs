pub mod text_input;

pub use blit_widget::{interact, popover};

pub mod resize {
    pub use blit_widget::resize::*;

    pub type ResizeState = blit_widget::resize::ResizeState<f32>;
}

pub mod scroll {
    pub use blit_widget::scroll::*;

    pub type ScrollState = blit_widget::scroll::ScrollState<f32>;
    pub type ScrollListState = blit_widget::scroll::ScrollListState<f32>;
}

pub mod split {
    pub use blit_widget::split::*;

    pub type SplitState = blit_widget::split::SplitState<f32>;
}
pub use blit_widget::layout;
