use blit::{Animation, AnimationOptions, Axis, Rect, Scalar, Ui, state::NoLayout};

use super::{CONTENT, Goal, Request, ScrollLayout, ScrollState, Scrollbar, animate, build_scroll, update};
use crate::Context;

blit::builder! {
    /// use for arbitrary content such as forms and small collections
    /// builds all content every frame
    pub struct ScrollArea<'a, T: Scalar> {
        new(state: &'a mut ScrollState<T>),
        axis: Axis = Axis::Vertical,
        scroll_speed: f32 = 1.0,
        inertia_friction: f32 = 2.5,
        minimum_thumb_extent: T = T::from_f32(1.0),
        /// scrollbars cover the content instead of taking space beside it
        floating: bool = false,
    }
}

impl<T: Scalar> ScrollArea<'_, T> {
    pub fn build<C, S, W, R, H>(self, ui: Ui<'_, C, S>, content: W, track: R, thumb: H)
    where
        C: Context<Scalar = T>,
        S: NoLayout,
        W: FnOnce(Ui<'_, C>),
        R: FnOnce(Ui<'_, C>),
        H: FnOnce(Ui<'_, C>, bool),
    {
        ui.build(|mut ui: Ui<'_, C>| {
            let Self {
                state,
                axis,
                scroll_speed,
                inertia_friction,
                minimum_thumb_extent,
                floating,
            } = self;
            let (thumb_active, viewport_known) = update(state, &mut ui, axis, scroll_speed, inertia_friction);
            // a focus change reveals the focused node wherever it is
            if ui.focus_moved()
                && let Some(focused) = ui.focus_within()
            {
                let content = ui.node(ui.id().child(CONTENT)).area();
                if let Some(content) = content
                    && let Some(node) = ui.node(focused).area()
                {
                    let offset = reveal_offset(state, axis, content, node);
                    state.scroll_to(offset, Animation::default(), AnimationOptions::default());
                }
            }
            let reveal = match state.request.target {
                Goal::Widget(id) => Some(id),
                _ => {
                    animate(state, &mut ui, viewport_known);
                    None
                }
            };
            let offset = state.offset;
            // searched from the content so the scrollbar can never match
            let mut revealed = None;
            let content = |mut ui: Ui<'_, C>| {
                if let Some(id) = reveal {
                    revealed = ui.area().zip(ui.node(ui.id().child(id)).area());
                }
                content(ui)
            };
            let mut viewport = build_scroll(
                ui,
                ScrollLayout {
                    axis,
                    offset,
                    viewport_extent: state.viewport_extent,
                    scrollbar: Scrollbar {
                        minimum: minimum_thumb_extent,
                        floating,
                    },
                },
                content,
                track,
                thumb,
                thumb_active,
                state.maximum_offset() > T::ZERO,
            );
            match (reveal, revealed) {
                (Some(_), Some((content, node))) => {
                    state.request.target = Goal::Offset(reveal_offset(state, axis, content, node));
                    animate(state, &mut viewport, viewport_known);
                    if state.offset != offset {
                        viewport.request_frame();
                    }
                }
                (Some(_), None) if std::mem::replace(&mut state.request.missed, true) => {
                    state.request = Request::default()
                }
                (Some(_), None) => viewport.request_frame(),
                (None, _) => {}
            }
        })
    }
}

/// the offset that moves the least to bring `node` into view, from previous layout areas
fn reveal_offset<T: Scalar>(state: &ScrollState<T>, axis: Axis, content: Rect<T>, node: Rect<T>) -> T {
    let start = match axis {
        Axis::Horizontal => node.x - content.x,
        Axis::Vertical => node.y - content.y,
    };
    let end = start.endpoint(axis.extent(node.size()));
    state.offset.clamp((end - state.viewport_extent).min(start), start)
}
