pub mod performance;
pub mod popover;
pub mod resize;
pub mod scroll;
pub mod split;
pub mod text_input;

pub use blit::{Content, Ui, Widget, WidgetId};

#[macro_export]
macro_rules! export {
    ($coord:ty, $context:ty, $clip:expr) => {
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
            use $crate::{Content, Ui, Widget};

            pub type Behavior = $crate::scroll::area::Behavior<$coord>;
            pub type Config = $crate::scroll::area::Config<$coord>;
            pub type State = $crate::scroll::area::State<$coord>;

            pub fn new<'a, C, B, T, H>(
                state: &'a mut State,
                config: Config,
                content: C,
                scrollbar: B,
            ) -> impl Widget<$context> + 'a
            where
                C: Widget<$context> + 'a,
                B: FnOnce(bool) -> (Option<T>, Option<H>) + 'a,
                T: Content<$context>,
                H: Content<$context>,
            {
                move |ui: Ui<'_, $context>| $crate::scroll::area::build(ui, state, config, $clip, content, scrollbar)
            }
        }
        pub mod scroll_list {
            use $crate::{Content, Ui, Widget, WidgetId};

            pub type Behavior = $crate::scroll::list::Behavior<$coord>;
            pub type Config = $crate::scroll::list::Config<$coord>;
            pub type State = $crate::scroll::list::State<$coord>;

            pub fn new<'a, I, K, F, B, T, H>(
                state: &'a mut State,
                config: Config,
                items: I,
                widget_id: K,
                item: F,
                scrollbar: B,
            ) -> impl Widget<$context> + 'a
            where
                I: ExactSizeIterator + 'a,
                K: FnMut(&I::Item) -> WidgetId + 'a,
                F: FnMut(Ui<'_, $context>, I::Item) + 'a,
                B: FnOnce(bool) -> (Option<T>, Option<H>) + 'a,
                T: Content<$context>,
                H: Content<$context>,
            {
                move |ui: Ui<'_, $context>| {
                    $crate::scroll::list::build(ui, state, config, items, widget_id, item, $clip, scrollbar)
                }
            }
        }
        pub mod virtual_list {
            pub use $crate::scroll::virtual_list::Response;
            use $crate::{Content, Ui, Widget, WidgetId};
            pub type Behavior = $crate::scroll::virtual_list::Behavior<$coord>;
            pub type Config = $crate::scroll::virtual_list::Config<$coord>;
            pub type State = $crate::scroll::virtual_list::State<$coord>;

            pub fn new<'a, R, K, F, B, T, H>(
                state: &'a mut State,
                rows: &'a [R],
                config: Config,
                widget_id: K,
                item: F,
                scrollbar: B,
            ) -> impl Widget<$context, Response = Response> + 'a
            where
                K: FnMut(&R) -> WidgetId + 'a,
                F: FnMut(Ui<'_, $context>, &R) + 'a,
                B: FnOnce(bool) -> (Option<T>, Option<H>) + 'a,
                T: Content<$context>,
                H: Content<$context>,
            {
                move |ui: Ui<'_, $context>| {
                    $crate::scroll::virtual_list::build(ui, state, rows, $clip, config, scrollbar, widget_id, item)
                }
            }
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
