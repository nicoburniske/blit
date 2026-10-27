mod drag;
mod performance;
mod popover;
mod resize;
mod scroll;
mod split;
mod text_input;

pub use blit::{Clip, Context, Id, Ui, Widget, state};
pub use drag::{Drag, DragResponse};
pub use performance::{FrameProfiler, Stage};
pub use popover::{Popover, PopoverClose, PopoverState};
pub use resize::{Resize, ResizeEdge, ResizeGrip, ResizeState};
pub use scroll::{
    ScrollArea, ScrollState, UniformList, UniformListResponse, VirtualList, VirtualListResponse, VirtualListState,
};
pub use split::{Split, SplitDivider, SplitState};
pub use text_input::{Response as TextInputResponse, State as TextInputState};

#[cfg(test)]
mod test {
    pub struct TestContext;

    impl blit::Context for TestContext {
        type Scalar = f32;
    }
}
