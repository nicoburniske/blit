mod performance;
mod popover;
mod resize;
mod scroll;
mod split;
mod text_input;

pub use blit::{Clip, Context, Ui, Widget, WidgetId, state};
pub use performance::{FrameProfiler, Stage};
pub use popover::{Popover, PopoverClose, PopoverState};
pub use resize::{Resize, ResizeEdge, ResizeGrip, ResizeState};
pub use scroll::{
    ScrollArea, ScrollState, UniformList, UniformListResponse, VirtualList, VirtualListResponse, VirtualListState,
};
pub use split::{Split, SplitState};
pub use text_input::{Response as TextInputResponse, State as TextInputState};

#[cfg(test)]
mod test {
    pub struct TestContext;

    impl blit::Context for TestContext {
        type Scalar = f32;
    }

    #[derive(Clone, Copy)]
    pub struct TestClip;

    impl blit::Clip<TestContext> for TestClip {
        fn push(&self, _: &mut TestContext, _: blit::Rect<f32>) {}

        fn pop(&self, _: &mut TestContext) {}
    }
}
