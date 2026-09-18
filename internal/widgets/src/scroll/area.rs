use blit::{Axis, Clip, Content, Ui, Widget};

pub use super::shared::{Behavior, State};

use super::shared::{ScrollLayout, build_scroll, update};

blit::builder! {
    #[derive(Clone, Copy, Debug)]
    pub struct Config {
        new(),
        axis: Axis = Axis::Vertical,
        behavior: Behavior = Behavior::default(),
    }
}

pub fn build<C, W, X, T, H>(
    mut ui: Ui<'_, C>,
    state: &mut State,
    area: Config,
    clip: X,
    content: W,
    scrollbar: impl FnOnce(bool) -> (Option<T>, Option<H>),
) where
    W: Widget<C>,
    X: Clip<C>,
    T: Content<C>,
    H: Content<C>,
{
    let config = area.behavior;
    let axis = area.axis;
    let (thumb_active, _) = update(state, &mut ui, axis, config);
    let (track, thumb) = scrollbar(thumb_active);
    build_scroll(
        ui,
        state.id,
        ScrollLayout {
            axis,
            offset: state.offset,
            scrollbar_thickness: config.scrollbar_thickness,
            minimum_thumb_extent: config.minimum_thumb_extent,
        },
        clip,
        content,
        track,
        thumb,
    );
}
