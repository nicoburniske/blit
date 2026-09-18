use blit::{Interaction, Widget};

use crate::{GuiContext, Ui};

pub use blit_widgets::popover::{Close, Config, State};

pub fn show<'a, T, C>(
    state: &'a mut State,
    config: Config,
    trigger: T,
    content: C,
) -> impl Widget<GuiContext, Response = Option<C::Response>> + 'a
where
    T: FnOnce(Ui<'_>, Interaction, bool) + 'a,
    C: Widget<GuiContext> + 'a,
{
    move |ui: Ui<'_>| blit_widgets::popover::build(ui, state, config, trigger, content)
}
