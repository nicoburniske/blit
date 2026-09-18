pub use super::shared::{Behavior, State};

use super::shared::{ScrollLayout, build_scroll, update};
use blit::{Axis, Constraints, Layout, LayoutCx, Point, Size};
use blit::{Clip, Content, Platform, Ui};

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
///
/// the item callback receives each visible item and a fresh node
pub fn build<P, I, F, X, T, H>(
    mut ui: Ui<'_, P>,
    state: &mut State,
    list: Config,
    items: I,
    mut item: F,
    clip: X,
    scrollbar: impl FnOnce(bool) -> (Option<T>, Option<H>),
) where
    P: Platform,
    I: ExactSizeIterator,
    F: FnMut(Ui<'_, P>, I::Item),
    X: Clip<P>,
    T: Content<P>,
    H: Content<P>,
{
    let config = list.behavior;
    let axis = list.axis;
    let gap = list.gap;
    let item_extent = list.item_extent;
    assert!(item_extent.is_finite() && item_extent > 0.0);
    assert!(gap.is_finite() && gap >= 0.0);
    let res = ui.layout_resolution();
    let item_extent = res.extent(axis, item_extent);
    let gap = res.extent(axis, gap);
    let stride = item_extent + gap;
    let count = items.len();
    let (thumb_active, viewport_known) = update(state, &mut ui, axis, config);
    let viewport_extent = if viewport_known {
        state.viewport_extent
    } else {
        ui.request_frame();
        axis.extent(ui.screen().size())
    };
    let first = ((state.offset / stride).floor() as usize)
        .min(count)
        .saturating_sub(1);
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
    let content = move |ui: Ui<'_, P>| {
        let mut list = ui.layout(layout);
        for (offset, value) in items.enumerate() {
            item(list.child(first + offset), value);
        }
    };
    let (track, thumb) = scrollbar(thumb_active);
    build_scroll(
        ui,
        state.id,
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

impl<R: Platform> Layout<R> for ListLayout {
    type Item = usize;

    fn layout(&self, ui: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let mut cross_extent: f32 = 0.0;
        for child in ui.children() {
            let mut child_constraints = constraints;
            self.axis
                .set_extent(&mut child_constraints.min, self.item_extent);
            self.axis
                .set_extent(&mut child_constraints.max, self.item_extent);
            let size = ui.layout_child(child, child_constraints);
            let offset = *ui.item(child) as f32 * self.stride;
            cross_extent = cross_extent.max(self.axis.other().extent(size));
            match self.axis {
                Axis::Horizontal => {
                    ui.set_child_position(child, Point::new(offset, 0.0));
                }
                Axis::Vertical => {
                    ui.set_child_position(child, Point::new(0.0, offset));
                }
            }
        }
        constraints.constrain(match self.axis {
            Axis::Horizontal => Size::new(self.total_extent, cross_extent),
            Axis::Vertical => Size::new(cross_extent, self.total_extent),
        })
    }

    fn override_size(&self, _: &mut Self::Item, _: Option<f32>, _: Option<f32>) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use blit::{Frame, FrameInfo, LayoutResolution, Rect};

    use super::*;

    struct TestPlatform;

    impl Platform for TestPlatform {}

    #[derive(Clone, Copy)]
    struct TestClip;

    impl Clip<TestPlatform> for TestClip {
        fn push(&self, _: &mut TestPlatform, _: Rect) {}

        fn pop(&self, _: &mut TestPlatform) {}
    }

    #[test]
    fn list_builds_only_the_resolved_visible_range() {
        let mut frame = Frame::default();
        let mut platform = TestPlatform;
        let frame_info =
            FrameInfo::new(Size::new(80.0, 10.0)).layout_resolution(LayoutResolution::Discrete {
                step: Size::uniform(1.0),
            });
        let mut state = State::new();
        let mut built = Vec::new();

        frame.render(&mut platform, frame_info, |ui: Ui<'_, TestPlatform>| {
            build(
                ui,
                &mut state,
                Config::new(1.5),
                0..100,
                |ui, index| {
                    built.push(index);
                    ui.build(());
                },
                TestClip,
                |_| (None::<()>, None::<()>),
            )
        });
        assert_eq!(built, [0, 1, 2, 3, 4, 5]);

        built.clear();
        state.viewport_extent = 10.0;
        state.content_extent = 200.0;
        state.scroll_to(20.0);
        frame.render(&mut platform, frame_info, |ui: Ui<'_, TestPlatform>| {
            build(
                ui,
                &mut state,
                Config::new(1.5),
                0..100,
                |ui, index| {
                    built.push(index);
                    ui.build(());
                },
                TestClip,
                |_| (None::<()>, None::<()>),
            )
        });
        assert_eq!(built, [9, 10, 11, 12, 13, 14, 15]);
    }
}
