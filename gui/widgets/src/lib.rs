mod performance;
mod text_input;

pub use blit_gui::widget::{RichText, Text};
pub use blit_widgets::{
    Popover, PopoverClose, PopoverState, Resize, ResizeEdge, ScrollArea, ScrollList, ScrollListResponse, Split,
    VirtualList, VirtualListResponse,
};
pub type ResizeState = blit_widgets::ResizeState<f32>;
pub type ResizeGrip = blit_widgets::ResizeGrip<f32>;
pub type SplitState = blit_widgets::SplitState<f32>;
pub type ScrollAreaState = blit_widgets::ScrollAreaState<f32>;
pub type ScrollListState = blit_widgets::ScrollListState<f32>;
pub type VirtualListState = blit_widgets::VirtualListState<f32>;

pub use self::{
    performance::{Performance, PerformanceState},
    text_input::{TextInput, TextInputResponse, TextInputState},
};

pub mod layout {
    blit_layout::export!(f32);
}
