use blit::{Axis, Clip, Scalar, Target, Ui, Widget};

use super::{Goal, Request, ScrollLayout, ScrollState, Scrollbar, animate, build_scroll, update};
use crate::Context;

blit::builder! {
    /// use for arbitrary content such as forms and small collections
    /// builds all content every frame
    pub struct ScrollArea<'a, T: Scalar> {
        new(state: &'a mut ScrollState<T>),
        axis: Axis = Axis::Vertical,
        scroll_speed: f32 = 1.0,
        inertia_friction: f32 = 2.5,
        scrollbar_thickness: T = T::from_f32(1.0),
        minimum_thumb_extent: T = T::from_f32(1.0),
        clip<X = ()>: X = (),
    }
}

impl<T: Scalar, X> ScrollArea<'_, T, X> {
    /// creates a widget without a scrollbar
    pub fn build<C: Context<Scalar = T>>(self, content: impl FnOnce(Ui<'_, C>)) -> impl Widget<C, Response = ()>
    where
        X: Clip<C>,
    {
        move |ui: Ui<'_, C>| self.build_inner(ui, content, None::<fn(Ui<'_, C>)>, None::<fn(Ui<'_, C>, bool)>)
    }

    /// creates a widget with a track and thumb
    pub fn build_scrollbar<C: Context<Scalar = T>>(
        self,
        content: impl FnOnce(Ui<'_, C>),
        track: impl FnOnce(Ui<'_, C>),
        thumb: impl FnOnce(Ui<'_, C>, bool),
    ) -> impl Widget<C, Response = ()>
    where
        X: Clip<C>,
    {
        move |ui: Ui<'_, C>| self.build_inner(ui, content, Some(track), Some(thumb))
    }

    fn build_inner<C: Context<Scalar = T>>(
        self,
        mut ui: Ui<'_, C>,
        content: impl FnOnce(Ui<'_, C>),
        track: Option<impl FnOnce(Ui<'_, C>)>,
        thumb: Option<impl FnOnce(Ui<'_, C>, bool)>,
    ) where
        X: Clip<C>,
    {
        let Self {
            state,
            axis,
            scroll_speed,
            inertia_friction,
            scrollbar_thickness,
            minimum_thumb_extent,
            clip,
        } = self;
        let (thumb_active, viewport_known) = update(state, &mut ui, axis, scroll_speed, inertia_friction);
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
                revealed = ui.area().zip(ui.at(Target::THIS.at(id)).area());
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
                    thickness: scrollbar_thickness,
                    minimum: minimum_thumb_extent,
                },
            },
            clip,
            content,
            track,
            thumb,
            thumb_active,
        );
        match (reveal, revealed) {
            (Some(_), Some((content, widget))) => {
                let start = match axis {
                    Axis::Horizontal => widget.x - content.x,
                    Axis::Vertical => widget.y - content.y,
                };
                let end = start.endpoint(axis.extent(widget.size()));
                state.request.target =
                    Goal::Offset(state.offset.clamp((end - state.viewport_extent).min(start), start));
                animate(state, &mut viewport, viewport_known);
                if state.offset != offset {
                    viewport.request_frame();
                }
            }
            (Some(_), None) if std::mem::replace(&mut state.request.missed, true) => state.request = Request::default(),
            (Some(_), None) => viewport.request_frame(),
            (None, _) => {}
        }
    }
}
