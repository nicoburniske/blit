use blit::{Content, Widget, WidgetId};
pub use blit_widgets::scroll::list::{Behavior, Config, State};

use crate::{BoundsClip, GuiContext, Ui};

pub fn new<'a, I, K, F, B, T, H>(
    state: &'a mut State,
    config: Config,
    items: I,
    widget_id: K,
    item: F,
    scrollbar: B,
) -> impl Widget<GuiContext> + 'a
where
    I: ExactSizeIterator + 'a,
    K: FnMut(&I::Item) -> WidgetId + 'a,
    F: FnMut(Ui<'_>, I::Item) + 'a,
    B: FnOnce(bool) -> (Option<T>, Option<H>) + 'a,
    T: Content<GuiContext>,
    H: Content<GuiContext>,
{
    move |ui: Ui<'_>| {
        blit_widgets::scroll::list::build(ui, state, config, items, widget_id, item, BoundsClip, scrollbar)
    }
}
