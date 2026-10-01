use blit::{Axis, Clip, Content, Ui, Widget};
use blit_layout::Unit;

pub use crate::scroll::shared::{Behavior, State};
use crate::scroll::shared::{ScrollLayout, build_scroll, update};

blit::builder! {
    #[const]
    #[derive(Clone, Copy, Debug)]
    pub struct Config<U: Unit> {
        new(),
        axis: Axis = Axis::Vertical,
        behavior: Behavior<U> = Behavior::new(),
    }
}

pub fn build<C, U: Unit, W, X, T, H>(
    mut ui: Ui<'_, C>,
    state: &mut State,
    area: Config<U>,
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
    let (thumb_active, _) = update(&mut ui, state, axis, config);
    let (track, thumb) = scrollbar(thumb_active);
    build_scroll(
        ui,
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
