use std::ops::Range;

use blit::{Axis, Clip, Constraints, Context, Layout, LayoutCx, Point, Scalar, Sense, Size, Ui, Widget, WidgetId};

pub use super::State;
use super::{ScrollLayout, animate, build_scroll, update};

blit::builder! {
    /// scrolls uniform items while building only the visible range
    pub struct ScrollList<'a, T: Scalar> {
        new(state: &'a mut State<T>),
        items<I: ExactSizeIterator = std::iter::Empty<()>> = std::iter::empty(),
        axis: Axis = Axis::Vertical,
        gap: T = T::ZERO,
        item_extent: T = T::from_f32(1.0),
        scroll_speed: f32 = 1.0,
        inertia_friction: f32 = 6.0,
        sense: Sense = Sense::SCROLL,
        scrollbar_thickness: T = T::from_f32(1.0),
        minimum_thumb_extent: T = T::from_f32(1.0),
        widget_id<K: FnMut(&I::Item) -> WidgetId = ()> where I: ExactSizeIterator = (),
        item<F: FnMut(Ui<'_, C>, I::Item) = (), C: Context<Scalar = T>> where I: ExactSizeIterator = (),
        clip<X = ()> = (),
        #[into]
        track<A = ()>: Option<_> = None,
        #[into]
        thumb_with<H = fn(bool) -> ()>: Option<_> = None,
    }
}

impl<'a, T: Scalar, I, K, F, X, A, H> ScrollList<'a, T, I, K, F, X, A, H> {
    pub fn thumb<B>(self, thumb: B) -> ScrollList<'a, T, I, K, F, X, A, impl FnOnce(bool) -> B> {
        self.thumb_with(move |_| thumb)
    }
}

impl<C: Context<Scalar = T>, T: Scalar, I, K, F, X, A, H, B> Widget<C> for ScrollList<'_, T, I, K, F, X, A, H>
where
    I: ExactSizeIterator,
    K: FnMut(&I::Item) -> WidgetId,
    F: FnMut(Ui<'_, C>, I::Item),
    X: Clip<C>,
    A: Widget<C, blit::state::Node>,
    H: FnOnce(bool) -> B,
    B: Widget<C, blit::state::Node>,
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
            mut widget_id,
            mut item,
            clip,
            track,
            thumb_with,
        } = self;
        assert!(item_extent.is_finite() && item_extent > C::Scalar::ZERO);
        assert!(gap.is_finite() && gap >= C::Scalar::ZERO);
        let stride = item_extent.endpoint(gap);
        let count = items.len();
        let (thumb_active, viewport_known) = update(state, &mut ui, axis, sense, scroll_speed, inertia_friction);
        animate(state, &mut ui, axis, viewport_known);
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
        let total_extent = item_extent.repeat(count).endpoint(gap.repeat(count.saturating_sub(1)));
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
                let id = widget_id(&value);
                list.child()
                    .item(first + offset)
                    .widget_id(id)
                    .build(|ui: Ui<'_, C>| item(ui, value));
            }
        };
        let thumb = thumb_with.map(|thumb| thumb(thumb_active));
        let offset = state.offset;
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, Input, WidgetId};

    use super::*;
    use crate::test::{TestClip, TestContext};

    #[test]
    fn list_builds_only_the_resolved_visible_range() {
        fn layout(
            frame: &mut Frame<TestContext>,
            context: &mut TestContext,
            info: FrameInfo<f32>,
            state: &mut State<f32>,
            built: &mut Vec<(usize, WidgetId)>,
            swap: bool,
        ) {
            let mut rows: [usize; 100] = std::array::from_fn(|index| index);
            if swap {
                rows.swap(5, 6);
            }
            frame.build(context, info, Duration::ZERO, Input::None, |ui: Ui<'_, TestContext>| {
                ui.build(
                    ScrollList::new(state)
                        .items(rows.iter().enumerate())
                        .item_extent(2.0)
                        .widget_id(|row| WidgetId::new(("row", row.1)))
                        .item(|ui: Ui<'_, TestContext>, (index, _)| {
                            built.push((index, ui.current_widget_id()));
                            ui.build(());
                        })
                        .clip(TestClip),
                )
            });
            frame.layout(context);
        }

        let mut frame = Frame::default();
        let mut context = TestContext;
        let frame_info = FrameInfo::new(Size::new(80.0, 10.0));
        let mut state = State::new();
        let mut built = Vec::new();

        layout(&mut frame, &mut context, frame_info, &mut state, &mut built, false);
        assert!(built.iter().map(|row| row.0).eq(0..6));
        let (first_id, default_id) = (built[5].1, built[4].1);
        built.clear();
        state.viewport_extent = 10.0;
        state.content_extent = 200.0;
        state.scroll_to(8.0, None);
        layout(&mut frame, &mut context, frame_info, &mut state, &mut built, true);
        assert!(built.iter().map(|row| row.0).eq(3..10));
        assert_eq!(built[3].1, first_id);
        assert_eq!(built[1].1, default_id);
    }
}
