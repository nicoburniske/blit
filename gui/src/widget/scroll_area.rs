use blit::{Content, Widget};
pub use blit_widgets::scroll::area::{Behavior, Config, State};

use crate::{BoundsClip, GuiContext, Ui};

pub fn new<'a, C, B, T, H>(
    state: &'a mut State,
    config: Config,
    content: C,
    scrollbar: B,
) -> impl Widget<GuiContext> + 'a
where
    C: Widget<GuiContext> + 'a,
    B: FnOnce(bool) -> (Option<T>, Option<H>) + 'a,
    T: Content<GuiContext>,
    H: Content<GuiContext>,
{
    move |ui: Ui<'_>| blit_widgets::scroll::area::build(ui, state, config, BoundsClip, content, scrollbar)
}
