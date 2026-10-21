use std::ops::Range;

use blit::{Axis, Clip, Constraints, Context, Id, Layout, LayoutCx, Point, Scalar, Sense, Size, Ui, Widget};

use super::{ScrollLayout, Scrollbar, Target, animate, build_scroll, update};
use crate::ScrollState;

blit::builder! {
    /// use for large lists with a known fixed item height or width
    /// builds only the visible range and a small margin
    /// row ids default to hashed indices scoped to the list
    /// id callbacks receive the list id and return the row's absolute id
    pub struct UniformList<'a, T: Scalar, I>
    where I: ExactSizeIterator, I: Clone,
    {
        new(state: &'a mut ScrollState<T>, items: I),
        axis: Axis = Axis::Vertical,
        gap: T = T::ZERO,
        item_extent: T = T::from_f32(1.0),
        scroll_speed: f32 = 1.0,
        inertia_friction: f32 = 6.0,
        sense: Sense = Sense::SCROLL,
        scrollbar_thickness: T = T::from_f32(1.0),
        minimum_thumb_extent: T = T::from_f32(1.0),
        #[bounds(FnMut(Id, usize, &I::Item) -> Id)]
        id<K = fn(Id, usize, &<I as Iterator>::Item) -> Id>: K = |parent, index, _| parent.child(index),
        clip<X = ()>: X = (),
    }
}

impl<T: Scalar, I, K, X> UniformList<'_, T, I, K, X>
where
    I: ExactSizeIterator + Clone,
    K: FnMut(Id, usize, &I::Item) -> Id,
{
    /// creates a widget without a scrollbar
    pub fn build<C: Context<Scalar = T>>(
        self,
        item: impl FnMut(Ui<'_, C>, I::Item),
    ) -> impl Widget<C, Response = Response>
    where
        X: Clip<C>,
    {
        move |ui: Ui<'_, C>| self.build_inner(ui, item, None::<fn(Ui<'_, C>)>, None::<fn(Ui<'_, C>, bool)>)
    }

    /// creates a widget with a track and thumb
    pub fn build_scrollbar<C: Context<Scalar = T>>(
        self,
        item: impl FnMut(Ui<'_, C>, I::Item),
        track: impl FnOnce(Ui<'_, C>),
        thumb: impl FnOnce(Ui<'_, C>, bool),
    ) -> impl Widget<C, Response = Response>
    where
        X: Clip<C>,
    {
        move |ui: Ui<'_, C>| self.build_inner(ui, item, Some(track), Some(thumb))
    }

    fn build_inner<C: Context<Scalar = T>>(
        self,
        mut ui: Ui<'_, C>,
        mut item: impl FnMut(Ui<'_, C>, I::Item),
        track: Option<impl FnOnce(Ui<'_, C>)>,
        thumb: Option<impl FnOnce(Ui<'_, C>, bool)>,
    ) -> Response
    where
        X: Clip<C>,
    {
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
            clip,
        } = self;
        assert!(item_extent.is_finite() && item_extent > C::Scalar::ZERO);
        assert!(gap.is_finite() && gap >= C::Scalar::ZERO);
        let id = ui.current_id();
        let stride = item_extent.endpoint(gap);
        let count = items.len();
        let (thumb_active, viewport_known) = update(state, &mut ui, axis, sense, scroll_speed, inertia_friction);
        let total_extent = item_extent.repeat(count).endpoint(gap.repeat(count.saturating_sub(1)));
        state.content_extent = total_extent;
        // resolve offscreen rows by index before choosing which rows to build
        if viewport_known && let Target::Widget(target) = state.request.target {
            state.request.target = match items
                .clone()
                .enumerate()
                .position(|(index, value)| item_id(id, index, &value) == target)
            {
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
                let row_id = item_id(id, first + offset, &value);
                list.child()
                    .item(first + offset)
                    .id(row_id)
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
