use blit::{Widget, WidgetId};

use crate::{TuiPlatform, Ui};

pub use blit_widgets::resize::{Config, Edge, Grip, State};

pub fn area<'a, C, F, G>(
    state: &'a mut State,
    id: WidgetId,
    config: Config,
    content: C,
    grip: F,
) -> impl Widget<TuiPlatform> + 'a
where
    C: Widget<TuiPlatform> + 'a,
    F: FnMut(Grip) -> G + 'a,
    G: Widget<TuiPlatform>,
{
    move |ui: Ui<'_>| blit_widgets::resize::build(ui, state, id, config, content, grip)
}
