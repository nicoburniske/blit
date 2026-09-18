use blit::{Content, Widget};

use crate::{BoundsClip, TuiPlatform, Ui};

pub use blit_widgets::scroll::area::{Behavior, Config, State};

pub fn new<'a, C, B, T, H>(
    state: &'a mut State,
    config: Config,
    content: C,
    scrollbar: B,
) -> impl Widget<TuiPlatform> + 'a
where
    C: Widget<TuiPlatform> + 'a,
    B: FnOnce(bool) -> (Option<T>, Option<H>) + 'a,
    T: Content<TuiPlatform>,
    H: Content<TuiPlatform>,
{
    move |ui: Ui<'_>| {
        blit_widgets::scroll::area::build(ui, state, config, BoundsClip, content, scrollbar)
    }
}
