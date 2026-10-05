use blit::{Axis, Clip, Content, Context, Scalar, Ui, Widget};

pub use super::{Behavior, State};
use super::{ScrollLayout, build_scroll, update};

blit::builder! {
    #[derive(Clone, Copy, Debug)]
    pub struct Config<T: Scalar> {
        new(),
        axis: Axis = Axis::Vertical,
        behavior: Behavior<T> = Behavior::default(),
    }
}

pub fn build<C: Context, W, X, T, H>(
    mut ui: Ui<'_, C>,
    state: &mut State<C::Scalar>,
    area: Config<C::Scalar>,
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
    let offset = state.offset;
    let (track, thumb) = scrollbar(thumb_active);
    build_scroll(
        ui,
        ScrollLayout {
            axis,
            offset: move |_| offset,
            scrollbar_thickness: config.scrollbar_thickness,
            minimum_thumb_extent: config.minimum_thumb_extent,
        },
        clip,
        content,
        track,
        thumb,
    );
}
