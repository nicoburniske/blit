use blit::{Content, Widget, WidgetId};
pub use blit_widgets::scroll::virtual_list::{Behavior, Config, Response, State};

use crate::{BoundsClip, TuiContext, Ui};

pub fn new<'a, R, K, F, B, T, H>(
    state: &'a mut State,
    rows: &'a [R],
    config: Config,
    widget_id: K,
    item: F,
    scrollbar: B,
) -> impl Widget<TuiContext, Response = Response> + 'a
where
    K: FnMut(&R) -> WidgetId + 'a,
    F: FnMut(Ui<'_>, &R) + 'a,
    B: FnOnce(bool) -> (Option<T>, Option<H>) + 'a,
    T: Content<TuiContext>,
    H: Content<TuiContext>,
{
    move |ui: Ui<'_>| {
        blit_widgets::scroll::virtual_list::build(ui, state, rows, BoundsClip, config, scrollbar, widget_id, item)
    }
}
