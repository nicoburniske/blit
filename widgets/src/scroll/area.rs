use std::time::Duration;

use blit::{Axis, Clip, Context, Easing, Scalar, Ui, Widget};

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
    T: Widget<C, blit::state::Node>,
    H: Widget<C, blit::state::Node>,
{
    let config = area.behavior;
    let axis = area.axis;
    let (thumb_active, viewport_known) = update(state, &mut ui, axis, config);
    let id = ui.current_widget_id();
    let target = state.reveal.and_then(|(target, _)| {
        ui.geometry(target)
            .zip(ui.geometry(id.child("content")))
            .filter(|_| viewport_known)
            .map(|(target, content)| {
                let start = match axis {
                    Axis::Horizontal => target.x - content.x,
                    Axis::Vertical => target.y - content.y,
                };
                let end = start.endpoint(axis.extent(target.size()));
                state
                    .offset
                    .clamp(
                        (end - state.viewport_extent).min(start).max(C::Scalar::ZERO),
                        start.max(C::Scalar::ZERO),
                    )
                    .min(state.maximum_offset())
            })
    });
    let (duration, easing) = state
        .reveal
        .and_then(|(_, animation)| animation)
        .unwrap_or((Duration::ZERO, Easing::Linear));
    state.offset = C::Scalar::from_f32(ui.animate(
        id.child("scroll into view"),
        target.unwrap_or(state.offset).to_f32(),
        duration,
        easing,
    ))
    .clamp(C::Scalar::ZERO, state.maximum_offset());
    if target == Some(state.offset) {
        state.reveal = None;
    } else if state.reveal.is_some() && target.is_none() {
        ui.request_frame();
    }
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
