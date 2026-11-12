mod performance;
pub mod text_input;

pub use blit_gui::text::{RichText, Text};
pub use blit_widgets::{
    interact,
    popover::{Popover, PopoverClose, PopoverState},
    resize::{Resize, ResizeEdge},
    scroll::{
        area::ScrollArea,
        uniform_list::{Response as UniformListResponse, UniformList},
        virtual_list::{Response as VirtualListResponse, VirtualList},
    },
    split::{Split, SplitDivider},
};
pub type ResizeState = blit_widgets::resize::ResizeState<f32>;
pub use blit_widgets::resize::ResizeGrip;
pub type SplitState = blit_widgets::split::SplitState<f32>;
pub type ScrollState = blit_widgets::scroll::ScrollState<f32>;
pub type VirtualListState = blit_widgets::scroll::virtual_list::VirtualListState<f32>;

pub use self::{
    performance::{Performance, PerformanceState},
    text_input::{TextInput, TextInputResponse, TextInputState},
};

pub mod layout {
    blit_layout::export!(f32);
}
