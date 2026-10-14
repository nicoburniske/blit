mod performance;
pub use blit_widgets::{
    Popover, PopoverClose, PopoverState, Resize, ResizeEdge, ScrollArea, ScrollList, ScrollListResponse, Split, Stage,
    VirtualList, VirtualListResponse,
};
pub type ResizeState = blit_widgets::ResizeState<i32>;
pub type ResizeGrip = blit_widgets::ResizeGrip<i32>;
pub type SplitState = blit_widgets::SplitState<i32>;
pub type ScrollAreaState = blit_widgets::ScrollAreaState<i32>;
pub type ScrollListState = blit_widgets::ScrollListState<i32>;
pub type VirtualListState = blit_widgets::VirtualListState<i32>;

mod block;
mod text;
mod text_input;

pub use self::{
    block::{Block, Title},
    performance::{Performance, PerformanceState},
    text::Text,
    text_input::{TextInput, TextInputResponse, TextInputState},
};
