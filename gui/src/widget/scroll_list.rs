use blit::{Content, Widget};

use crate::{BoundsClip, GuiContext, Ui};

pub use blit_widgets::scroll::list::{Behavior, Config, State};

pub fn new<'a, I, F, B, T, H>(
    state: &'a mut State,
    config: Config,
    items: I,
    item: F,
    scrollbar: B,
) -> impl Widget<GuiContext> + 'a
where
    I: ExactSizeIterator + 'a,
    F: FnMut(Ui<'_>, I::Item) + 'a,
    B: FnOnce(bool) -> (Option<T>, Option<H>) + 'a,
    T: Content<GuiContext>,
    H: Content<GuiContext>,
{
    move |ui: Ui<'_>| {
        blit_widgets::scroll::list::build(ui, state, config, items, item, BoundsClip, scrollbar)
    }
}
