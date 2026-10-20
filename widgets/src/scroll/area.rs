use blit::{Axis, Clip, Context, Scalar, Sense, Ui, Widget};

pub use super::ScrollState;
use super::{ScrollLayout, Scrollbar, Target, animate, build_scroll, update};

blit::builder! {
    /// use for arbitrary content such as forms and small collections
    /// builds all content every frame
    pub struct ScrollArea<'a, C: Context> {
        new(state: &'a mut ScrollState<C::Scalar>),
        content<W = ()>: W = (),
        axis: Axis = Axis::Vertical,
        scroll_speed: f32 = 1.0,
        inertia_friction: f32 = 6.0,
        sense: Sense = Sense::SCROLL,
        scrollbar_thickness: C::Scalar = C::Scalar::from_f32(1.0),
        minimum_thumb_extent: C::Scalar = C::Scalar::from_f32(1.0),
        clip<X = ()>: X = (),
        #[into]
        track<A: FnOnce(Ui<'_, C>) = fn(Ui<'_, C>)>: Option<A> = None,
        #[into]
        thumb<H: FnOnce(Ui<'_, C>, bool) = fn(Ui<'_, C>, bool)>: Option<H> = None,
    }
}

impl<C: Context, W, X, A, H> Widget<C> for ScrollArea<'_, C, W, X, A, H>
where
    W: Widget<C>,
    X: Clip<C>,
    A: FnOnce(Ui<'_, C>),
    H: FnOnce(Ui<'_, C>, bool),
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
