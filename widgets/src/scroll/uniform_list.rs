use std::ops::Range;

use blit::{Axis, Clip, Constraints, Context, Id, Layout, LayoutCx, Point, Scalar, Sense, Size, Ui, Widget};

use super::{ScrollLayout, Scrollbar, Target, animate, build_scroll, update};
use crate::ScrollState;

blit::builder! {
    /// use for large lists with a known fixed item height or width
    /// builds only the visible range and a small margin
    pub struct UniformList<'a, C: Context>
    where I: ExactSizeIterator, I: Clone,
    {
        new(state: &'a mut ScrollState<C::Scalar>),
        items<I = std::iter::Empty<()>>: I = std::iter::empty(),
        axis: Axis = Axis::Vertical,
        gap: C::Scalar = C::Scalar::ZERO,
        item_extent: C::Scalar = C::Scalar::from_f32(1.0),
        scroll_speed: f32 = 1.0,
        inertia_friction: f32 = 6.0,
        sense: Sense = Sense::SCROLL,
        scrollbar_thickness: C::Scalar = C::Scalar::from_f32(1.0),
        minimum_thumb_extent: C::Scalar = C::Scalar::from_f32(1.0),
        id<K: FnMut(&I::Item) -> Id = ()>: K = (),
        item<F: FnMut(Ui<'_, C>, I::Item) = ()>: F = (),
        clip<X = ()>: X = (),
        #[into]
        track<A: FnOnce(Ui<'_, C>) = fn(Ui<'_, C>)>: Option<A> = None,
        #[into]
        thumb<H: FnOnce(Ui<'_, C>, bool) = fn(Ui<'_, C>, bool)>: Option<H> = None,
    }
}

impl<C: Context, I, K, F, X, A, H> Widget<C> for UniformList<'_, C, I, K, F, X, A, H>
where
    I: ExactSizeIterator + Clone,
    K: FnMut(&I::Item) -> Id,
    F: FnMut(Ui<'_, C>, I::Item),
    X: Clip<C>,
    A: FnOnce(Ui<'_, C>),
    H: FnOnce(Ui<'_, C>, bool),
{
    type Response = Response;

    fn build(self, mut ui: Ui<'_, C>) -> Response {
        let Self {
            state,
            items,
            axis,
            gap,
            item_extent,
            scroll_speed,
            inertia_friction,
            sense,
            scrollbar_thickness,
            minimum_thumb_extent,
            id: mut item_id,
            mut item,
            clip,
            track,
            thumb,
        } = self;
        assert!(item_extent.is_finite() && item_extent > C::Scalar::ZERO);
        assert!(gap.is_finite() && gap >= C::Scalar::ZERO);
        let stride = item_extent.endpoint(gap);
        let count = items.len();
        let (thumb_active, viewport_known) = update(state, &mut ui, axis, sense, scroll_speed, inertia_friction);
        let total_extent = item_extent.repeat(count).endpoint(gap.repeat(count.saturating_sub(1)));
        state.content_extent = total_extent;
        // resolve offscreen rows by index before choosing which rows to build
        if viewport_known && let Target::Widget(target) = state.request.target {
            state.request.target = match items.clone().position(|value| item_id(&value) == target) {
                Some(index) => {
                    let start = stride.repeat(index);
                    let end = start.endpoint(item_extent);
                    let minimum = (end - state.viewport_extent).min(start);
                    let maximum = start;
                    // move only as far as needed to reveal the row
                    Target::Offset(state.offset.clamp(minimum, maximum))
                }
                None => Target::None,
            };
        }
        animate(state, &mut ui, axis, viewport_known);
        state.offset = state.offset.clamp(C::Scalar::ZERO, state.maximum_offset());
        let viewport_extent = if viewport_known {
            state.viewport_extent
        } else {
            ui.request_frame();
            axis.extent(ui.screen().size())
        };
        let start = state.offset.index(stride).min(count);
        let extent = state.offset.endpoint(viewport_extent);
        let end = extent.index(stride);
        let end = end.saturating_add(usize::from(stride.repeat(end) < extent)).min(count);
        let start = start
            .saturating_add(usize::from(stride.repeat(start).endpoint(item_extent) <= state.offset))
            .min(end);
        let visible_range = if viewport_extent > C::Scalar::ZERO {
            start..end
        } else {
            start..start
        };
        let first = start.saturating_sub(1);
        let end = end.saturating_add(1).min(count);
        let layout = ListLayout {
            axis,
            item_extent,
            stride,
            total_extent,
        };
        let items = items.skip(first).take(end - first);
        let content = move |ui: Ui<'_, C>| {
            let mut list = ui.layout(layout);
            for (offset, value) in items.enumerate() {
                let id = item_id(&value);
                list.child()
                    .item(first + offset)
                    .id(id)
                    .build(|ui: Ui<'_, C>| item(ui, value));
            }
        };
        let offset = state.offset;
        build_scroll(
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
        Response { visible_range }
    }
}

pub struct Response {
    /// row indices intersecting the viewport excluding overscan
    pub visible_range: Range<usize>,
}

#[derive(Clone, Copy)]
struct ListLayout<T> {
    axis: Axis,
    item_extent: T,
    stride: T,
    total_extent: T,
}

impl<C: Context<Scalar = T>, T: Scalar> Layout<C> for ListLayout<T> {
    type Item = usize;

    fn layout(&self, ui: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints<T>) -> Size<T> {
        let mut cross_extent = T::ZERO;
        for child in ui.children() {
            let mut child_constraints = constraints;
            self.axis.set_extent(&mut child_constraints.min, self.item_extent);
            self.axis.set_extent(&mut child_constraints.max, self.item_extent);
            let size = ui.layout_child(child, child_constraints);
            cross_extent = cross_extent.max(self.axis.other().extent(size));
            let offset = self.stride.repeat(*ui.item(child));
            ui.set_child_position(
                child,
                match self.axis {
                    Axis::Horizontal => Point::new(offset, T::ZERO),
                    Axis::Vertical => Point::new(T::ZERO, offset),
                },
            );
        }
        constraints.constrain(match self.axis {
            Axis::Horizontal => Size::new(self.total_extent, cross_extent),
            Axis::Vertical => Size::new(cross_extent, self.total_extent),
        })
    }
}
