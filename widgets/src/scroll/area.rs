use blit::{Axis, Clip, Context, Scalar, Sense, Ui, Widget};

pub use super::ScrollAreaState;
use super::{ScrollLayout, Target, animate, build_scroll, update};

blit::builder! {
    pub struct ScrollArea<'a, T: Scalar> {
        new(state: &'a mut ScrollAreaState<T>),
        content<W = ()>: W = (),
        axis: Axis = Axis::Vertical,
        scroll_speed: f32 = 1.0,
        inertia_friction: f32 = 6.0,
        sense: Sense = Sense::SCROLL,
        scrollbar_thickness: T = T::from_f32(1.0),
        minimum_thumb_extent: T = T::from_f32(1.0),
        clip<X = ()>: X = (),
        #[into]
        track<A = ()>: Option<A> = None,
        #[into]
        thumb<H = fn(bool) -> ()>: Option<H> = None,
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
            thumb,
        } = self;
        let (thumb_active, viewport_known) = update(state, &mut ui, axis, sense, scroll_speed, inertia_friction);
        animate(state, &mut ui, axis, viewport_known);
        let offset = state.offset;
        let thumb = thumb.map(|thumb| thumb(thumb_active));
        let reveal = match state.request.target {
            Target::Widget(id) => Some(id),
            _ => None,
        };
        if !build_scroll(
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
            reveal,
        ) {
            state.request.target = Target::None;
        }
    }
}
