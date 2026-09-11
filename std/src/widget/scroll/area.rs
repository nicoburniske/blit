use super::{Config, NoScrollbar, ScrollLayout, Scrollbar, State, build_scroll, update};
use blit::Axis;
use blit::{Clip, Content, Platform, Ui, Widget};
use std::marker::PhantomData;

/// scrolls one widget along one axis
pub struct Area<'a, R, X, S = NoScrollbar, C = ()> {
    state: &'a mut State,
    clip: X,
    content: C,
    scrollbar: S,
    config: Config,
    axis: Axis,
    marker: PhantomData<fn() -> R>,
}

impl<'a, R, X, S> Area<'a, R, X, S>
where
    R: Platform,
    S: Default + Scrollbar,
{
    pub fn new(state: &'a mut State, clip: X) -> Self {
        let scrollbar = S::default();
        let config = scrollbar.config();
        Self {
            state,
            clip,
            content: (),
            scrollbar,
            config,
            axis: Axis::Vertical,
            marker: PhantomData,
        }
    }

    /// sets the scrollable content
    pub fn build<C>(self, content: C) -> Area<'a, R, X, S, C>
    where
        C: Widget<R>,
    {
        Area {
            state: self.state,
            clip: self.clip,
            content,
            scrollbar: self.scrollbar,
            config: self.config,
            axis: self.axis,
            marker: PhantomData,
        }
    }
}

impl<R, X, S, C> Area<'_, R, X, S, C> {
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }

    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }
}

impl<R, C, X, S> Widget<R> for Area<'_, R, X, S, C>
where
    R: Platform,
    C: Widget<R>,
    X: Clip<R>,
    S: Scrollbar,
    S::Track: Content<R>,
    S::Thumb: Content<R>,
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, R>) {
        let (thumb_active, _) = update::<_, S>(self.state, &mut ui, self.axis, self.config);
        build_scroll(
            ui,
            self.state.id,
            ScrollLayout {
                axis: self.axis,
                offset: self.state.offset,
                scrollbar_thickness: self.config.scrollbar_thickness,
                minimum_thumb_extent: self.config.minimum_thumb_extent,
            },
            self.clip,
            self.content,
            self.scrollbar,
            thumb_active,
        );
    }
}
