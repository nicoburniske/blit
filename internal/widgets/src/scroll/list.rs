use blit::{Axis, Clip, Constraints, Content, Layout, LayoutCx, Point, Size, Ui, WidgetId};
use blit_layout::round;

pub use super::shared::{Behavior, State};
use super::shared::{ScrollLayout, build_scroll, update};

blit::builder! {
    #[derive(Clone, Copy, Debug)]
    pub struct Config {
        new(item_extent: f32),
        axis: Axis = Axis::Vertical,
        gap: f32 = 0.0,
        behavior: Behavior = Behavior::default(),
    }
}

/// scrolls uniform items while building only the visible range
pub fn build<C, I, K, F, X, T, H>(
    mut ui: Ui<'_, C>,
    state: &mut State,
    list: Config,
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
    assert!(item_extent.is_finite() && item_extent > 0.0);
    assert!(gap.is_finite() && gap >= 0.0);
    let item_extent = round(item_extent);
    let gap = round(gap);
    let stride = item_extent + gap;
    let count = items.len();
    let (thumb_active, viewport_known) = update(state, &mut ui, axis, config);
    let viewport_extent = if viewport_known {
        state.viewport_extent
    } else {
        ui.request_frame();
        axis.extent(ui.screen().size())
    };
    let first = ((state.offset / stride).floor() as usize).min(count).saturating_sub(1);
    let end = (((state.offset + viewport_extent) / stride).ceil() as usize)
        .saturating_add(1)
        .min(count);
    let total_extent = count as f32 * item_extent + count.saturating_sub(1) as f32 * gap;
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
    build_scroll(
        ui,
        ScrollLayout {
            axis,
            offset: state.offset,
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
struct ListLayout {
    axis: Axis,
    item_extent: f32,
    stride: f32,
    total_extent: f32,
}

impl<C> Layout<C> for ListLayout {
    type Item = usize;

    fn layout(&self, ui: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints) -> Size {
        let mut cross_extent: f32 = 0.0;
        for child in ui.children() {
            let mut child_constraints = constraints;
            self.axis.set_extent(&mut child_constraints.min, self.item_extent);
            self.axis.set_extent(&mut child_constraints.max, self.item_extent);
            let size = ui.layout_child(child, child_constraints);
            let offset = *ui.item(child) as f32 * self.stride;
            cross_extent = cross_extent.max(self.axis.other().extent(size));
            match self.axis {
                Axis::Horizontal => {
                    ui.set_position(child, Point::new(offset, 0.0));
                }
                Axis::Vertical => {
                    ui.set_position(child, Point::new(0.0, offset));
                }
            }
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
            info: FrameInfo,
            state: &mut State,
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
