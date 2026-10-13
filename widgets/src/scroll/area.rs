use blit::{Axis, Clip, Context, Scalar, Sense, Ui, Widget};

pub use super::State;
use super::{ScrollLayout, animate, build_scroll, update};

blit::builder! {
    pub struct ScrollArea<'a, T: Scalar> {
        new(state: &'a mut State<T>),
        content<W = ()> = (),
        axis: Axis = Axis::Vertical,
        scroll_speed: f32 = 1.0,
        inertia_friction: f32 = 6.0,
        sense: Sense = Sense::SCROLL,
        scrollbar_thickness: T = T::from_f32(1.0),
        minimum_thumb_extent: T = T::from_f32(1.0),
        clip<X = ()> = (),
        #[into]
        track<A = ()>: Option<_> = None,
        #[into]
        thumb_with<H = fn(bool) -> ()>: Option<_> = None,
    }
}

impl<'a, T: Scalar, W, X, A, H> ScrollArea<'a, T, W, X, A, H> {
    pub fn thumb<B>(self, thumb: B) -> ScrollArea<'a, T, W, X, A, impl FnOnce(bool) -> B> {
        self.thumb_with(move |_| thumb)
    }
}

impl<C: Context<Scalar = T>, T: Scalar, W, X, A, H, B> Widget<C> for ScrollArea<'_, T, W, X, A, H>
where
    W: Widget<C>,
    X: Clip<C>,
    A: Widget<C, blit::state::Node>,
    H: FnOnce(bool) -> B,
    B: Widget<C, blit::state::Node>,
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, C>) {
        let Self {
            state,
            content,
            axis,
            scroll_speed,
            inertia_friction,
            sense,
            scrollbar_thickness,
            minimum_thumb_extent,
            clip,
            track,
            thumb_with,
        } = self;
        let (thumb_active, viewport_known) = update(state, &mut ui, axis, sense, scroll_speed, inertia_friction);
        animate(state, &mut ui, axis, viewport_known);
        let offset = state.offset;
        let thumb = thumb_with.map(|thumb| thumb(thumb_active));
        build_scroll(
            ui,
            ScrollLayout {
                axis,
                offset: move |_| offset,
                scrollbar_thickness,
                minimum_thumb_extent,
            },
            clip,
            content,
            track,
            thumb,
        );
    }
}
