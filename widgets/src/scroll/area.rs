use blit::{Axis, Clip, Context, Scalar, Ui, Widget};

pub use super::ScrollState;
use super::{ScrollLayout, Scrollbar, Target, animate, build_scroll, update};

blit::builder! {
    /// use for arbitrary content such as forms and small collections
    /// builds all content every frame
    pub struct ScrollArea<'a, T: Scalar> {
        new(state: &'a mut ScrollState<T>),
        axis: Axis = Axis::Vertical,
        scroll_speed: f32 = 1.0,
        inertia_friction: f32 = 6.0,
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
            Target::Widget(id) => Some(id),
            _ => {
                animate(state, &mut ui, axis, viewport_known);
                None
            }
        };
        let offset = state.offset;
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
        match reveal {
            Some(id) if !viewport.contains_widget(id) => state.request.target = Target::None,
            Some(_) => {
                animate(state, &mut viewport, axis, viewport_known);
                if state.offset != offset {
                    viewport.request_frame();
                }
            }
            None => {}
        }
    }
}
