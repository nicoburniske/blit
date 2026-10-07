use blit::{Axis, Clip, Constraints, Content, Context, Layout, LayoutCx, Point, Scalar, Size, Ui, WidgetId};

pub use super::{Behavior, State};
use super::{ScrollLayout, build_scroll, update};

blit::builder! {
    #[derive(Clone, Copy, Debug)]
    pub struct Config<T: Scalar> {
        new(item_extent: T),
        axis: Axis = Axis::Vertical,
        gap: T = T::ZERO,
        behavior: Behavior<T> = Behavior::default(),
    }
}

/// scrolls uniform items while building only the visible range
pub fn build<C: Context, I, K, F, X, T, H>(
    mut ui: Ui<'_, C>,
    state: &mut State<C::Scalar>,
    list: Config<C::Scalar>,
    items: I,
    mut widget_id: K,
    mut item: F,
    clip: X,
    scrollbar: impl FnOnce(bool) -> (Option<T>, Option<H>),
) where
    I: ExactSizeIterator,
    K: FnMut(&I::Item) -> WidgetId,
    F: FnMut(Ui<'_, C>, I::Item),
    X: Clip<C>,
    T: Content<C>,
    H: Content<C>,
{
    let config = list.behavior;
    let axis = list.axis;
    let gap = list.gap;
    let item_extent = list.item_extent;
    assert!(item_extent.is_finite() && item_extent > C::Scalar::ZERO);
    assert!(gap.is_finite() && gap >= C::Scalar::ZERO);
    let stride = item_extent.endpoint(gap);
    let count = items.len();
    let (thumb_active, viewport_known) = update(state, &mut ui, axis, config);
    let viewport_extent = if viewport_known {
        state.viewport_extent
    } else {
        ui.request_frame();
        axis.extent(ui.screen().size())
    };
    let first = state.offset.index(stride).min(count).saturating_sub(1);
    let extent = state.offset.endpoint(viewport_extent);
    let end = extent.index(stride);
    let end = end
        .saturating_add(usize::from(stride.repeat(end) < extent))
        .saturating_add(1)
        .min(count);
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
    let (track, thumb) = scrollbar(thumb_active);
    let offset = state.offset;
    build_scroll(
        ui,
        ScrollLayout {
            axis,
            offset: move |_| offset,
            scrollbar_thickness: config.scrollbar_thickness,
            minimum_thumb_extent: config.minimum_thumb_extent,
        },
        clip,
        content,
        track,
        thumb,
    );
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
        ) {
            let swap = state.offset > 0.0;
            let mut rows: [usize; 100] = std::array::from_fn(|index| index);
            if swap {
                rows.swap(5, 6);
            }
            frame.build(context, info, Duration::ZERO, Input::None, |ui: Ui<'_, TestContext>| {
                build(
                    ui,
                    state,
                    Config::new(2.0),
                    rows.iter().enumerate(),
                    |row| WidgetId::new(("row", row.1)),
                    |ui, (index, _)| {
                        built.push((index, ui.current_widget_id()));
                        ui.build(());
                    },
                    TestClip,
                    |_| (None::<()>, None::<()>),
                )
            });
            frame.layout(context);
        }

        let mut frame = Frame::default();
        let mut context = TestContext;
        let frame_info = FrameInfo::new(Size::new(80.0, 10.0));
        let mut state = State::new();
        let mut built = Vec::new();

        layout(&mut frame, &mut context, frame_info, &mut state, &mut built);
        assert!(built.iter().map(|row| row.0).eq(0..6));
        let (first_id, default_id) = (built[5].1, built[4].1);
        built.clear();
        state.viewport_extent = 10.0;
        state.content_extent = 200.0;
        state.scroll_to(8.0);
        layout(&mut frame, &mut context, frame_info, &mut state, &mut built);
        assert!(built.iter().map(|row| row.0).eq(3..10));
        assert_eq!(built[3].1, first_id);
        assert_eq!(built[1].1, default_id);
    }
}
