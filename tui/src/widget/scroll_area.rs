use blit::{Content, Widget};
pub use blit_widgets::scroll::area::State;
pub type Behavior = blit_widgets::scroll::area::Behavior<u16>;
pub type Config = blit_widgets::scroll::area::Config<u16>;

use crate::{BoundsClip, TuiContext, Ui};

pub fn new<'a, C, B, T, H>(
    state: &'a mut State,
    config: Config,
    content: C,
    scrollbar: B,
) -> impl Widget<TuiContext> + 'a
where
    C: Widget<TuiContext> + 'a,
    B: FnOnce(bool) -> (Option<T>, Option<H>) + 'a,
    T: Content<TuiContext>,
    H: Content<TuiContext>,
{
    move |ui: Ui<'_>| blit_widgets::scroll::area::build(ui, state, config, BoundsClip, content, scrollbar)
}
