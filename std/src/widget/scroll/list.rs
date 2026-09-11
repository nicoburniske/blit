use super::{Config, NoScrollbar, ScrollLayout, Scrollbar, State, build_scroll, update};
use blit::{Axis, Constraints, Layout, LayoutCx, Point, Size};
use blit::{Clip, Content, Platform, Ui, Widget};
use std::marker::PhantomData;

/// scrolls uniform items while building only the visible range
///
/// the item callback receives each visible item and a fresh node
pub struct List<'a, R, I, X, S = NoScrollbar, F = ()> {
    state: &'a mut State,
    clip: X,
    items: I,
    item: F,
    scrollbar: S,
    config: Config,
    axis: Axis,
    item_extent: f32,
    gap: f32,
    marker: PhantomData<fn() -> R>,
}

impl<'a, R, I, X, S> List<'a, R, I, X, S>
where
    R: Platform,
    I: ExactSizeIterator,
    S: Default + Scrollbar,
{
    pub fn new(state: &'a mut State, clip: X, items: I, item_extent: f32) -> Self {
        let scrollbar = S::default();
        let config = scrollbar.config();
        Self {
            state,
            clip,
            items,
            item: (),
            scrollbar,
            config,
            axis: Axis::Vertical,
            item_extent,
            gap: 0.0,
            marker: PhantomData,
        }
    }

    /// sets the item builder
    pub fn build<F>(self, item: F) -> List<'a, R, I, X, S, F>
    where
        F: FnMut(Ui<'_, R>, I::Item),
    {
        List {
            state: self.state,
            clip: self.clip,
            items: self.items,
            item,
            scrollbar: self.scrollbar,
            config: self.config,
            axis: self.axis,
            item_extent: self.item_extent,
            gap: self.gap,
            marker: PhantomData,
        }
    }
}

impl<R, I, X, S, F> List<'_, R, I, X, S, F> {
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }
}

impl<R, I, F, X, S> Widget<R> for List<'_, R, I, X, S, F>
where
    R: Platform,
    I: ExactSizeIterator,
    F: FnMut(Ui<'_, R>, I::Item),
    X: Clip<R>,
    S: Scrollbar,
    S::Track: Content<R>,
    S::Thumb: Content<R>,
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, R>) {
        assert!(self.item_extent.is_finite() && self.item_extent > 0.0);
        assert!(self.gap.is_finite() && self.gap >= 0.0);
        let res = ui.layout_resolution();
        let item_extent = res.extent(self.axis, self.item_extent);
        let gap = res.extent(self.axis, self.gap);
        let stride = item_extent + gap;
        let count = self.items.len();
        let (thumb_active, viewport_known) =
            update(self.state, &mut ui, self.axis, self.config, S::HAS_THUMB);
        let viewport_extent = if viewport_known {
            self.state.viewport_extent
        } else {
            ui.request_frame();
            self.axis.extent(ui.screen().size())
        };
        let first = ((self.state.offset / stride).floor() as usize)
            .min(count)
            .saturating_sub(1);
        let end = (((self.state.offset + viewport_extent) / stride).ceil() as usize)
            .saturating_add(1)
            .min(count);
        let total_extent = count as f32 * item_extent + count.saturating_sub(1) as f32 * gap;
        let layout = ListLayout {
            axis: self.axis,
            item_extent,
            stride,
            total_extent,
        };
        let items = self.items.skip(first).take(end - first);
        let mut item = self.item;
        let content = move |ui: Ui<'_, R>| {
            let mut list = ui.layout(layout);
            for (offset, value) in items.enumerate() {
                item(list.child(first + offset), value);
            }
        };
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
            content,
            self.scrollbar,
            thumb_active,
        );
    }
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

    impl Platform for TestPlatform {
        fn begin(&mut self, _: FrameInfo) {}

        fn end(&mut self) {}
    }

    #[derive(Clone, Copy)]
    struct TestClip;

    impl Clip<TestPlatform> for TestClip {
        fn push(&self, _: &mut TestPlatform, _: Rect) {}

        fn pop(&self, _: &mut TestPlatform) {}
    }

    type TestList<'a, I, F = ()> = List<'a, TestPlatform, I, TestClip, NoScrollbar, F>;

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

        frame.render(
            &mut platform,
            frame_info,
            TestList::new(&mut state, TestClip, 0..100, 1.5).build(|ui, index| {
                built.push(index);
                ui.build(());
            }),
        );
        assert_eq!(built, [0, 1, 2, 3, 4, 5]);

        built.clear();
        state.viewport_extent = 10.0;
        state.content_extent = 200.0;
        state.scroll_to(20.0);
        frame.render(
            &mut platform,
            frame_info,
            TestList::new(&mut state, TestClip, 0..100, 1.5).build(|ui, index| {
                built.push(index);
                ui.build(());
            }),
        );
        assert_eq!(built, [9, 10, 11, 12, 13, 14, 15]);
    }
}
