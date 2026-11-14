pub mod performance;
pub mod text_input;

pub use blit_widgets::{interact, popover};

pub mod resize {
    pub use blit_widgets::resize::*;

    pub type ResizeState = blit_widgets::resize::ResizeState<f32>;
}

pub mod scroll {
    pub use blit_widgets::scroll::*;

    pub type ScrollState = blit_widgets::scroll::ScrollState<f32>;

    pub mod virtual_list {
        pub use blit_widgets::scroll::virtual_list::*;

        pub type VirtualListState = blit_widgets::scroll::virtual_list::VirtualListState<f32>;
    }
}

pub mod split {
    pub use blit_widgets::split::*;

    pub type SplitState = blit_widgets::split::SplitState<f32>;
}
pub use blit_layout as layout;
