pub mod performance;
pub mod popover;
pub mod resize;
pub mod scroll;
pub mod split;
pub mod text_input;

pub use blit::{Context, Ui, Widget, WidgetId, state};

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
            use $crate::{Context, Ui, Widget, state};

            pub type Behavior = $crate::scroll::area::Behavior<$coord>;
            pub type Config = $crate::scroll::area::Config<$coord>;
            pub type State = $crate::scroll::area::State<$coord>;

            pub fn new<'a, C, W, B, T, H>(
                state: &'a mut State,
                config: Config,
                content: W,
                scrollbar: B,
            ) -> impl Widget<C> + 'a
            where
                C: Context<Scalar = $coord> + AsMut<$context>,
                W: Widget<C> + 'a,
                B: FnOnce(bool) -> (Option<T>, Option<H>) + 'a,
                T: Widget<C, state::Node>,
                H: Widget<C, state::Node>,
            {
                move |ui: Ui<'_, C>| $crate::scroll::area::build(ui, state, config, $clip, content, scrollbar)
            }
        }
        pub mod scroll_list {
            pub use $crate::scroll::list::Response;
            use $crate::{Context, Ui, Widget, WidgetId, state};

            pub type Behavior = $crate::scroll::list::Behavior<$coord>;
            pub type Config = $crate::scroll::list::Config<$coord>;
            pub type State = $crate::scroll::list::State<$coord>;

            pub fn new<'a, C, I, K, F, B, T, H>(
                state: &'a mut State,
                config: Config,
                items: I,
                widget_id: K,
                item: F,
                scrollbar: B,
            ) -> impl Widget<C, Response = Response> + 'a
            where
                C: Context<Scalar = $coord> + AsMut<$context>,
                I: ExactSizeIterator + 'a,
                K: FnMut(&I::Item) -> WidgetId + 'a,
                F: FnMut(Ui<'_, C>, I::Item) + 'a,
                B: FnOnce(bool) -> (Option<T>, Option<H>) + 'a,
                T: Widget<C, state::Node>,
                H: Widget<C, state::Node>,
            {
                move |ui: Ui<'_, C>| {
                    $crate::scroll::list::build(ui, state, config, items, widget_id, item, $clip, scrollbar)
                }
            }
        }
        pub mod virtual_list {
            pub use $crate::scroll::virtual_list::Response;
            use $crate::{Context, Ui, Widget, WidgetId, state};
            pub type Behavior = $crate::scroll::virtual_list::Behavior<$coord>;
            pub type Config = $crate::scroll::virtual_list::Config<$coord>;
            pub type State = $crate::scroll::virtual_list::State<$coord>;

            pub fn new<'a, C, R, K, F, B, T, H>(
                state: &'a mut State,
                rows: &'a [R],
                config: Config,
                widget_id: K,
                item: F,
                scrollbar: B,
            ) -> impl Widget<C, Response = Response> + 'a
            where
                C: Context<Scalar = $coord> + AsMut<$context>,
                K: FnMut(&R) -> WidgetId + 'a,
                F: FnMut(Ui<'_, C>, &R) + 'a,
                B: FnOnce(bool) -> (Option<T>, Option<H>) + 'a,
                T: Widget<C, state::Node>,
                H: Widget<C, state::Node>,
            {
                move |ui: Ui<'_, C>| {
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
