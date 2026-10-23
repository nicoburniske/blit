mod performance;
pub use blit_widgets::{
    Popover, PopoverClose, PopoverState, Resize, ResizeEdge, ScrollArea, Split, Stage, UniformList,
    UniformListResponse, VirtualList, VirtualListResponse,
};
pub type ResizeState = blit_widgets::ResizeState<i32>;
pub type ResizeGrip = blit_widgets::ResizeGrip<i32>;
pub type SplitState = blit_widgets::SplitState<i32>;
pub type ScrollState = blit_widgets::ScrollState<i32>;
pub type VirtualListState = blit_widgets::VirtualListState<i32>;

mod block;
mod text;
mod text_input;

pub use self::{
    block::{Block, Title},
    performance::{Performance, PerformanceState},
    text::{Text, TextSource},
    text_input::{TextInput, TextInputResponse, TextInputState},
};
