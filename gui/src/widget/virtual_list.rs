use blit::{Content, Widget, WidgetId};

use crate::{BoundsClip, GuiContext, Ui};

pub use blit_widgets::scroll::virtual_list::{Behavior, Config, Response, State};

pub fn new<'a, R, K, F, B, T, H>(
    state: &'a mut State,
    rows: &'a [R],
    config: Config,
    key: K,
    item: F,
    scrollbar: B,
) -> impl Widget<GuiContext, Response = Response> + 'a
where
    K: FnMut(&R) -> WidgetId + 'a,
    F: FnMut(Ui<'_>, &R) + 'a,
    B: FnOnce(bool) -> (Option<T>, Option<H>) + 'a,
    T: Content<GuiContext>,
    H: Content<GuiContext>,
{
    move |ui: Ui<'_>| {
        blit_widgets::scroll::virtual_list::build(
            ui, state, rows, BoundsClip, config, scrollbar, key, item,
        )
    }
}
