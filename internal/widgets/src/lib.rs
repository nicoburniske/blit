pub mod performance;
pub mod popover;
pub mod resize;
pub mod scroll;
pub mod split;
pub mod text_input;

#[cfg(test)]
mod test {
    pub struct TestContext;

    #[derive(Clone, Copy)]
    pub struct TestClip;

    impl blit::Clip<TestContext> for TestClip {
        fn push(&self, _: &mut TestContext, _: blit::Rect) {}

        fn pop(&self, _: &mut TestContext) {}
    }
}
