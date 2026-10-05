pub mod performance;
pub mod popover;
pub mod resize;
pub mod scroll;
pub mod split;
pub mod text_input;

#[cfg(test)]
mod test {
    pub struct TestContext;

    impl blit_layout::Context for TestContext {
        fn round(value: f32) -> f32 {
            value.round()
        }

        fn allocate(cursor: &mut f32, share: f32) -> f32 {
            let start = Self::round(*cursor);
            *cursor += share;
            Self::round(*cursor) - start
        }
    }

    #[derive(Clone, Copy)]
    pub struct TestClip;

    impl blit::Clip<TestContext> for TestClip {
        fn push(&self, _: &mut TestContext, _: blit::Rect) {}

        fn pop(&self, _: &mut TestContext) {}
    }
}
