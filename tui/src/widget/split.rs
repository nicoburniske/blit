use blit::{Axis, Interaction, Widget, WidgetId};

use crate::{TuiContext, Ui};

pub use blit_widgets::split::{Config, State};

pub trait Divider {
    type Widget: Widget<TuiContext>;

    fn into_widget(self, axis: Axis, interaction: Interaction) -> Self::Widget;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoDivider;

impl Divider for NoDivider {
    type Widget = ();

    fn into_widget(self, _: Axis, _: Interaction) {}
}

pub fn pane<'a, L, T, D>(
    state: &'a mut State,
    id: WidgetId,
    config: Config,
    divider: D,
    leading: L,
    trailing: T,
) -> impl Widget<TuiContext> + 'a
where
    L: Widget<TuiContext> + 'a,
    T: Widget<TuiContext> + 'a,
    D: Divider + 'a,
{
    move |ui: Ui<'_>| {
        blit_widgets::split::build(
            ui,
            state,
            id,
            config,
            leading,
            trailing,
            |axis, interaction| divider.into_widget(axis, interaction),
        )
    }
}
