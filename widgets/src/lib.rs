pub mod performance;
pub mod popover;
pub mod resize;
pub mod scroll;
pub mod split;
pub mod text_input;

pub use blit::{Clip, Context, Ui, Widget, WidgetId, state};

#[macro_export]
macro_rules! export {
    ($coord:ty) => {
        pub use $crate::scroll::{ScrollArea, ScrollList, VirtualList};

        pub mod resize {
            pub use $crate::resize::{Edge, new};
            pub type Config = $crate::resize::Config<$coord>;
            pub type State = $crate::resize::State<$coord>;
            pub type Grip = $crate::resize::Grip<$coord>;
        }
        pub mod split {
            pub use $crate::split::new;
            pub type Config = $crate::split::Config<$coord>;
            pub type State = $crate::split::State<$coord>;
        }
        pub mod popover {
            pub use $crate::popover::{Close, State, new};
            pub type Config = $crate::popover::Config<$coord>;
        }
        pub mod scroll_area {
            pub type State = $crate::scroll::area::State<$coord>;
        }
        pub mod scroll_list {
            pub use $crate::scroll::list::Response;
            pub type State = $crate::scroll::list::State<$coord>;
        }
        pub mod virtual_list {
            pub use $crate::scroll::virtual_list::Response;
            pub type State = $crate::scroll::virtual_list::State<$coord>;
        }
    };
}

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
