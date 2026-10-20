use std::ops::Range;

use blit::{Axis, Clip, Constraints, Context, Id, Layout, LayoutCx, Point, Scalar, Sense, Size, Sizing, Ui, Widget};

use super::{Scrollbar, ScrollbarInput, ThumbProgress, content_motion, scrollbar_input};

blit::builder! {
    /// use for large vertical lists with variable or unknown row heights
    /// measures a bounded viewport window and estimates remaining heights
    /// rows must have positive finite heights
    /// callbacks identify and build rows by index
    /// row keys default to hashed indices and are scoped to the list
    /// an absent item callback renders an empty list
    /// an absent count scrolls indefinitely with a thumb covering the explored range
    pub struct VirtualList<'a, C: Context> {
        new(state: &'a mut VirtualListState<C::Scalar>),
        #[into]
        count: Option<usize> = None,
        scroll_speed: f32 = 1.0,
        inertia_friction: f32 = 6.0,
        sense: Sense = Sense::SCROLL,
        scrollbar_thickness: C::Scalar = C::Scalar::from_f32(1.0),
        minimum_thumb_extent: C::Scalar = C::Scalar::from_f32(1.0),
        #[bounds(FnMut(usize) -> Id)]
        id<K = fn(usize) -> Id>: K = Id::new,
        #[into]
        #[bounds(FnMut(Ui<'_, C>, usize))]
        item<F = fn(Ui<'_, C>, usize)>: Option<F> = None,
        #[into]
        clip<X = ()>: Option<X> = None,
        #[into]
        track<A = fn(Ui<'_, C>)>: Option<A> = None,
        #[into]
        thumb<H = fn(Ui<'_, C>, bool)>: Option<H> = None,
    }
}

impl<C: Context, K, F, X, A, H> Widget<C> for VirtualList<'_, C, K, F, X, A, H>
where
    K: FnMut(usize) -> Id,
    F: FnMut(Ui<'_, C>, usize),
    X: Clip<C>,
    A: FnOnce(Ui<'_, C>),
    H: FnOnce(Ui<'_, C>, bool),
{
    type Response = Response;

    fn build(self, mut ui: Ui<'_, C>) -> Response {
        const VIEWPORTS: usize = 2;

        let Self {
            state,
            count,
            scroll_speed,
            inertia_friction,
            sense,
            scrollbar_thickness,
            minimum_thumb_extent,
            id: mut item_id,
            item,
            clip,
            track,
            thumb,
        } = self;
        let Some(mut item) = item else {
            state.sample.clear();
            state.visible = 0..0;
            return Response::default();
        };
        let id = ui.current_id();
        let viewport = ui.geometry(id);
        let size = viewport.map_or(ui.screen().size(), |area| area.size());
        let overscan = size.height.repeat(VIEWPORTS);
        let row = state.sample.get(state.visible.start);
        let interaction = ui.interact(sense);
        let (thumb_active, scrollbar) = scrollbar_input(&mut state.motion, &mut ui, Axis::Vertical, size.height);
        let movement = match scrollbar {
            None => content_motion(
                &mut state.motion,
                &mut ui,
                Axis::Vertical,
                scroll_speed,
                inertia_friction,
                interaction,
            ),
            Some(_) => 0.0,
        };
        if viewport.is_none() {
            ui.request_frame();
        }
        // treat large offsets into an unmeasured window as movement so the existing seek logic estimates the row
        if state.sample.is_empty() && state.within.to_f32().abs() > overscan.to_f32() {
            state.scroll_delta = state.within.endpoint(state.scroll_delta);
            state.within = C::Scalar::ZERO;
        }
        let delta = {
            let delta: C::Scalar = state.motion.pixel_delta(movement);
            let delta = delta.endpoint(std::mem::take(&mut state.scroll_delta));
            if delta.to_f32().abs() > overscan.to_f32() && state.average == 0.0 {
                // wait for the initial heights before estimating a large jump
                state.scroll_delta = delta;
                ui.request_frame();
                C::Scalar::ZERO
            } else {
                delta
            }
        };
        let current = state.first as f64
            + row.map_or(0.0, |row| {
                (state.within.to_f32() / row.height.to_f32()).clamp(0.0, 1.0) as f64
            });
        let seek = match scrollbar {
            None | Some(ScrollbarInput::Idle | ScrollbarInput::Thumb(0.0)) => {
                if delta.to_f32().abs() > overscan.to_f32()
                    && state.average > 0.0
                    && !row.zip(state.sample.last()).is_some_and(|(row, last)| {
                        let offset = row.top.endpoint(state.within).endpoint(delta);
                        offset >= C::Scalar::ZERO && offset <= last.top.endpoint(last.height)
                    })
                {
                    Some((current + delta.to_f32() as f64 / state.average as f64).max(0.0))
                } else {
                    None
                }
            }
            Some(ScrollbarInput::Thumb(value) | ScrollbarInput::Track(value)) => {
                state.motion.remainder = 0.0;
                let offset = row.map_or(C::Scalar::ZERO, |row| row.top.endpoint(state.within));
                let (position, visible) = metrics(state, count, size.height, offset);
                let total = count.map_or(state.scrollbar.explored, |count| count as f64);
                let maximum = (total - visible).max(0.0);
                let previous = if count.is_some() {
                    position
                } else {
                    state.scrollbar.position
                };
                let position = if matches!(scrollbar, Some(ScrollbarInput::Track(_))) {
                    value as f64 * maximum
                } else {
                    (previous + value as f64 * maximum).clamp(0.0, maximum)
                };
                Some(match count {
                    Some(count) if position == maximum => count as f64,
                    Some(_) => position,
                    None if position == 0.0 => 0.0,
                    None => (current + position - previous).max(0.0),
                })
            }
        };
        let direction = seek.map_or(delta.to_f32() as f64, |position| position - current);
        if let Some(position) = seek {
            state.first = position as usize;
            state.within = C::Scalar::ZERO;
        }

        let limit = count.unwrap_or(usize::MAX);
        state.first = state.first.min(limit.saturating_sub(1));
        let anchor = Anchor {
            index: state.first,
            within: state.within,
            delta,
            fraction: seek.map(|position| (position - state.first as f64).clamp(0.0, 1.0) as f32),
        };
        let gutter = if track.is_some() {
            scrollbar_thickness.max(C::Scalar::ZERO).min(size.width)
        } else {
            C::Scalar::ZERO
        };
        let width = (size.width - gutter).max(C::Scalar::ZERO);
        let bounds = Constraints {
            min: Size::new(width, C::Scalar::ZERO),
            max: Size::new(width, C::Scalar::UNBOUNDED),
        };
        let mut content = ui.layout(VirtualLayout {
            anchor,
            count,
            size,
            row_bounds: bounds,
            scrollbar: Scrollbar {
                thickness: scrollbar_thickness,
                minimum: minimum_thumb_extent,
            },
        });
        if let Some(clip) = clip {
            content = content.clip(clip);
        }
        state.sample.clear();
        // measure rows while extending the window around the anchor
        let window = {
            let mut window = Window {
                first: anchor.index,
                end: anchor.index,
                within: anchor.within,
                ..Window::default()
            };
            let mut read_row = |index, window: &mut Window<C::Scalar>| {
                let row_id = item_id(index);
                let (_, size) = content
                    .child()
                    .item(Item::Row(index))
                    .id(id.child(row_id))
                    .build_measured(bounds, |ui: Ui<'_, C>| item(ui, index));
                assert!(size.height.is_finite() && size.height > C::Scalar::ZERO);
                state.sample.push(Row {
                    id: row_id,
                    top: window.push(anchor, index, size.height),
                    height: size.height,
                });
            };
            while window.end < limit
                && (window.end == anchor.index
                    || window.after_anchor < window.within.endpoint(overscan).max(size.height))
            {
                read_row(window.end, &mut window);
            }
            while window.first > 0
                && (-window.before_anchor > window.within
                    || (window.end == limit && window.after_anchor.endpoint(window.before_anchor) < overscan))
            {
                read_row(window.first - 1, &mut window);
            }
            window
        };
        let first = window.first;
        if first < anchor.index {
            state.sample.reverse();
            state.sample[anchor.index - first..].reverse();
            for row in &mut state.sample {
                row.top = row.top.endpoint(window.before_anchor);
            }
        }
        let extent = window.after_anchor.endpoint(window.before_anchor);
        let offset = window.offset(size.height, count);
        if (first == 0 && offset == C::Scalar::ZERO && movement < 0.0)
            || (window.end == limit && offset == (extent - size.height).max(C::Scalar::ZERO) && movement > 0.0)
        {
            state.motion.velocity = 0.0;
            state.motion.remainder = 0.0;
        }
        state.visible = state
            .sample
            .partition_point(|row| row.top.endpoint(row.height) <= offset)
            ..state
                .sample
                .partition_point(|row| row.top < offset.endpoint(size.height));
        if let Some(row) = state.sample.get(state.visible.start) {
            state.first = first + state.visible.start;
            state.within = offset - row.top;
        } else {
            state.first = first;
            state.within = C::Scalar::ZERO;
        }
        if !state.sample.is_empty() {
            state.average = extent.to_f32() / state.sample.len() as f32;
            if state.scrollbar.scale == 0.0 {
                state.scrollbar.scale = state.average;
            }
        }
        let (position, visible) = metrics(state, count, size.height, offset);
        let progress = {
            let scrollbar = &mut state.scrollbar;
            if position == 0.0 {
                scrollbar.position = 0.0;
            } else if direction > 0.0 || scrollbar.position == 0.0 {
                scrollbar.position = scrollbar.position.max(position);
            } else if direction < 0.0 {
                scrollbar.position = scrollbar.position.min(position);
            }
            // publish measurement corrections only while scrolling in that direction
            let position = if count.is_some() { position } else { scrollbar.position };
            scrollbar.explored = scrollbar.explored.max(position + visible * VIEWPORTS as f64);
            let total = count.map_or(scrollbar.explored, |count| count as f64);
            let scrollable = total - visible;
            ThumbProgress {
                position: if scrollable > 0.0 {
                    (position / scrollable) as f32
                } else {
                    0.0
                },
                visible_fraction: if total > 0.0 {
                    (visible / total).clamp(0.0, 1.0) as f32
                } else {
                    1.0
                },
            }
        };

        let pointer_row = if let Some((pointer, viewport)) = content.pointer_position().zip(viewport)
            && !state.sample.is_empty()
        {
            let target = offset.endpoint((pointer.y - viewport.y).clamp(C::Scalar::ZERO, viewport.height));
            Some(
                first
                    + state
                        .sample
                        .partition_point(|row| row.top.endpoint(row.height) <= target)
                        .min(state.sample.len() - 1),
            )
        } else {
            None
        };
        if let Some(track) = track {
            content
                .child()
                .item(Item::Track(progress))
                .id(id.child("scroll track"))
                .build(track);
        }
        if let Some(thumb) = thumb {
            content
                .child()
                .item(Item::Thumb(progress))
                .id(id.child("scroll thumb"))
                .build(|ui: Ui<'_, C>| thumb(ui, thumb_active));
        }
        let visible_range = first + state.visible.start..first + state.visible.end;
        if viewport.is_none() {
            // preserve the anchor until the viewport is known
            state.first = anchor.index;
            state.within = anchor.within.endpoint(anchor.delta);
            state.sample.clear();
            state.visible = 0..0;
            return Response {
                visible_range,
                ..Response::default()
            };
        }
        Response {
            pointer_row,
            visible_range,
            offset: position,
            visible_extent: visible,
            content_extent: count.map_or(state.scrollbar.explored, |count| count as f64),
        }
    }
}

#[derive(Default)]
pub struct Response {
    /// row indices intersecting the viewport excluding overscan
    pub visible_range: Range<usize>,
    /// row at the pointer y position clamped to the viewport edges
    pub pointer_row: Option<usize>,
    /// resolved position and visible extent in item coordinates
    pub offset: f64,
    pub visible_extent: f64,
    /// item count for finite lists or explored extent for unbounded lists
    pub content_extent: f64,
}

#[derive(Default)]
pub struct VirtualListState<T> {
    motion: super::Motion,

    // movement queued for the next build
    scroll_delta: T,

    // retain the viewport position when the window is replaced
    first: usize,
    within: T,

    // retain measurements only for the viewport window and estimate the rest
    sample: Vec<Row<T>>,
    visible: Range<usize>,
    average: f32,

    // fix the infinite scrollbar scale after the initial measurement
    scrollbar: ScrollbarState,
}

impl<T: Scalar> VirtualListState<T> {
    /// replaces the movement in pixels queued for the next build
    pub fn scroll_by(&mut self, amount: T) {
        self.scroll_delta = amount;
        self.motion.stop();
    }

    /// scrolls to a list index and pixel offset
    /// - nearby offsets use exact row measurements
    /// - large offscreen offsets use estimated row heights
    pub fn scroll_to(&mut self, index: usize, offset: T) {
        self.first = index;
        self.scrollbar.position = index as f64;
        self.within = offset;
        self.sample.clear();
        self.visible = 0..0;
        self.scroll_delta = T::ZERO;
        self.motion.stop();
    }

    /// returns the row index and pixel offset
    pub fn position(&self) -> (usize, T) {
        (self.first, self.within)
    }

    pub fn is_visible(&self, id: Id) -> bool {
        self.sample[self.visible.clone()].iter().any(|row| row.id == id)
    }
}

struct Row<T> {
    id: Id,
    top: T,
    height: T,
}

#[derive(Clone, Copy)]
struct Anchor<T> {
    index: usize,
    within: T,
    delta: T,
    fraction: Option<f32>,
}

#[derive(Default)]
struct Window<T> {
    first: usize,
    end: usize,
    after_anchor: T,
    before_anchor: T,
    within: T,
}

impl<T: Scalar> Window<T> {
    // rows arrive forward from the anchor then backward
    fn push(&mut self, anchor: Anchor<T>, index: usize, height: T) -> T {
        self.first = self.first.min(index);
        self.end = self.end.max(index + 1);
        if index == anchor.index {
            self.within = match anchor.fraction {
                Some(fraction) => T::from_f32(height.to_f32() * fraction),
                None => anchor.within.min(height).endpoint(anchor.delta),
            };
        }
        if index < anchor.index {
            self.before_anchor = self.before_anchor.endpoint(height);
            -self.before_anchor
        } else {
            let top = self.after_anchor;
            self.after_anchor = self.after_anchor.endpoint(height);
            top
        }
    }

    fn offset(&self, viewport: T, count: Option<usize>) -> T {
        let offset = self.within.endpoint(self.before_anchor);
        let offset = if self.first == 0 { offset.max(T::ZERO) } else { offset };
        if count == Some(self.end) {
            offset.min((self.after_anchor.endpoint(self.before_anchor) - viewport).max(T::ZERO))
        } else {
            offset
        }
    }
}

#[derive(Clone, Copy, Default)]
struct ScrollbarState {
    scale: f32,
    position: f64,
    explored: f64,
}

#[derive(Clone, Copy)]
enum Item {
    Row(usize),
    Track(ThumbProgress),
    Thumb(ThumbProgress),
}

impl Default for Item {
    fn default() -> Self {
        Self::Row(0)
    }
}

struct VirtualLayout<T> {
    anchor: Anchor<T>,
    count: Option<usize>,
    size: Size<T>,
    row_bounds: Constraints<T>,
    scrollbar: Scrollbar<T>,
}

impl<C: Context<Scalar = T>, T: Scalar> Layout<C> for VirtualLayout<T> {
    type Item = Item;

    fn sizing(&self, _axis: Axis) -> Sizing<T> {
        Sizing::grow()
    }

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints<T>) -> Size<T> {
        let width = if constraints.max.width.is_finite() {
            constraints.max.width
        } else {
            self.size.width
        };
        // both passes use the same row constraints until the viewport geometry catches up
        let window = {
            let mut window = Window {
                first: self.anchor.index,
                end: self.anchor.index,
                ..Window::default()
            };
            for child in cx.children() {
                if let Item::Row(index) = *cx.item(child) {
                    let height = cx.layout_child(child, self.row_bounds).height;
                    window.push(self.anchor, index, height);
                }
            }
            window
        };
        let content_extent = window.after_anchor.endpoint(window.before_anchor);
        let extent = content_extent.endpoint(if self.count == Some(window.end) {
            T::ZERO
        } else {
            self.size.height
        });
        let size = constraints.constrain(Size::new(width, extent));
        let offset = window.offset(size.height, self.count);
        {
            let mut positions = Window::default();
            for child in cx.children() {
                match *cx.item(child) {
                    Item::Row(index) => {
                        let height = cx.size(child).height;
                        let top = positions.push(self.anchor, index, height);
                        let top = top.endpoint(window.before_anchor);
                        let y = top - offset;
                        cx.set_child_position(child, Point::new(T::ZERO, y));
                    }
                    Item::Track(progress) => {
                        self.scrollbar
                            .layout(cx, Axis::Vertical, size, progress, Some(child), None);
                    }
                    Item::Thumb(progress) => {
                        self.scrollbar
                            .layout(cx, Axis::Vertical, size, progress, None, Some(child));
                    }
                }
            }
        }
        if size != self.size
            || offset < T::ZERO
            || content_extent - offset < size.height && self.count != Some(window.end)
        {
            cx.request_frame();
        }
        size
    }
}
fn metrics<T: Scalar>(state: &VirtualListState<T>, count: Option<usize>, height: T, offset: T) -> (f64, f64) {
    let position_at = |index, offset: T| {
        state.sample.get(index).map_or(state.first as f64, |row: &Row<T>| {
            state.first as f64 + index as f64 - state.visible.start as f64
                + ((offset - row.top).to_f32() / row.height.to_f32()).clamp(0.0, 1.0) as f64
        })
    };
    let position = position_at(state.visible.start, offset);
    let visible = if count.is_some() {
        position_at(state.visible.end.saturating_sub(1), offset.endpoint(height)) - position
    } else {
        let scale = if state.scrollbar.scale > 0.0 {
            state.scrollbar.scale
        } else {
            state.average
        };
        height.to_f32() as f64 / scale as f64
    };
    (position, visible)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, Input, Rect};

    use super::*;
    use crate::test::TestContext;

    #[test]
    fn shrinking_anchor_keeps_the_window_bounded() {
        for count in [Some(1_000_000), None] {
            let mut frame = Frame::default();
            let mut state = VirtualListState::default();
            let tall = |index| (Id::new(index), 10_000.0);
            settle_frame(&mut frame, &mut state, count, tall);
            state.scroll_by(5_000.0);
            settle_frame(&mut frame, &mut state, count, tall);
            render_frame(&mut frame, &mut state, count, Input::None, |index| {
                (Id::new(index), 1.0)
            });
            assert!(state.sample.len() <= 101);
            assert_eq!(state.position().0, 1);
            assert_eq!(frame.geometry(row_id(Id::new(1usize))).unwrap().y, 0.0);
            assert!(!frame.has_pending_redraw());
        }
    }

    #[test]
    fn queued_movement_and_position_preserve_the_row_offset() {
        for count in [Some(1_000_000), None] {
            let mut frame = Frame::default();
            let mut state = VirtualListState::default();
            state.scroll_by(5_000_000.0);
            settle_frame(&mut frame, &mut state, count, |index| (Id::new(index), 10.0));
            assert_eq!(state.position().0, 500_000);
            state.scroll_by(1.0);
            state.scroll_by(5.0);
            render_frame(&mut frame, &mut state, count, Input::None, |index| {
                (Id::new(index), 10.0)
            });
            let (index, offset) = state.position();
            assert_eq!(offset, 5.0);
            state.scroll_to(index + 1, offset);
            let (area, _) = render_frame(
                &mut frame,
                &mut state,
                count.map(|count| count + 1),
                Input::None,
                |index| {
                    let id = if index == 0 {
                        Id::new("head")
                    } else {
                        Id::new(index - 1)
                    };
                    (id, 10.0)
                },
            );
            assert_eq!(state.position(), (500_001, 5.0));
            assert!(state.is_visible(Id::new(500_000usize)));
            assert_eq!(area.unwrap().y, -5.0);
        }
    }

    #[test]
    fn thumb_end_seeks_the_finite_or_explored_boundary() {
        for (count, heights) in [
            (Some(1), &[1_000.0][..]),
            (Some(3), &[10.0, 10.0, 1_000.0]),
            (Some(20), &[10.0; 20]),
            (Some(3), &[10.0, 500.0, 10.0]),
            (Some(2), &[10.0, 10.0]),
            (None, &[10.0]),
        ] {
            let mut frame = Frame::default();
            let mut state = VirtualListState::default();
            settle_frame(&mut frame, &mut state, count, |index| {
                (Id::new(index), heights[index % heights.len()])
            });
            for input in [
                Input::PointerDown {
                    position: Point::new(79.5, 49.999),
                    button: blit::PointerButton::Primary,
                    modifiers: blit::Modifiers::NONE,
                },
                Input::PointerUp {
                    position: Point::new(79.5, 49.999),
                    button: blit::PointerButton::Primary,
                    modifiers: blit::Modifiers::NONE,
                    leave: false,
                },
            ] {
                render_frame(&mut frame, &mut state, count, input, |index| {
                    (Id::new(index), heights[index % heights.len()])
                });
            }
            if frame.has_pending_redraw() {
                settle_frame(&mut frame, &mut state, count, |index| {
                    (Id::new(index), heights[index % heights.len()])
                });
            }
            if let Some(count) = count {
                let area = frame.geometry(row_id(Id::new(count - 1))).unwrap();
                assert_eq!(area.y + area.height, heights.iter().sum::<f32>().min(50.0));
            } else {
                assert_eq!(state.position().0, 5);
                assert_eq!(state.within, 0.0);
            }
        }
    }

    #[test]
    fn variable_rows_keep_the_thumb_moving_with_the_scroll() {
        let mut frame = Frame::default();
        let mut state = VirtualListState::default();
        let row = |index| (Id::new(index), [1.0, 1024.0, 16.0, 512.0][index % 4]);
        let mut previous = settle_frame(&mut frame, &mut state, None, row).1;
        for delta in [16.0, -16.0] {
            for _ in 0..400 {
                state.scroll_by(delta);
                let thumb = render_frame(&mut frame, &mut state, None, Input::None, row).1;
                assert!((thumb.y - previous.y) * delta >= -0.0001);
                if delta > 0.0 {
                    assert!(thumb.height <= previous.height);
                }
                assert!(thumb.y >= 0.0 && thumb.y + thumb.height <= 50.00001);
                let idle = render_frame(&mut frame, &mut state, None, Input::None, row).1;
                assert_eq!(idle, thumb);
                assert!(!frame.has_pending_redraw());
                previous = thumb;
            }
        }
        assert_eq!(previous.y, 0.0);
    }

    #[test]
    fn scrolling_up_preserves_the_existing_row_position() {
        for count in [Some(1_000), None] {
            for (before, after) in [(1024.0, 16.0), (16.0, 1024.0)] {
                let mut frame = Frame::default();
                let mut state = VirtualListState::default();
                let row = |index| (Id::new(index), if index < 20 { before } else { after });
                state.scroll_to(20, 0.0);
                settle_frame(&mut frame, &mut state, count, row);
                let key = row_id(Id::new(20usize));
                let y = frame.geometry(key).unwrap().y;
                state.scroll_by(-20.0);
                render_frame(&mut frame, &mut state, count, Input::None, row);
                assert_eq!(frame.geometry(key).unwrap().y, y + 20.0);
                settle_frame(&mut frame, &mut state, count, row);
                assert_eq!(frame.geometry(key).unwrap().y, y + 20.0);
                state.scroll_by(40.0);
                render_frame(&mut frame, &mut state, count, Input::None, row);
                assert_eq!(frame.geometry(key).unwrap().y, y - 20.0);
            }
        }
    }

    fn settle_frame(
        frame: &mut Frame<TestContext>,
        state: &mut VirtualListState<f32>,
        count: Option<usize>,
        row: impl Fn(usize) -> (Id, f32),
    ) -> (Option<Rect<f32>>, Rect<f32>) {
        for _ in 0..8 {
            let result = render_frame(frame, state, count, Input::None, &row);
            if !frame.has_pending_redraw() {
                return result;
            }
        }
        panic!("virtual list did not converge");
    }

    fn render_frame(
        frame: &mut Frame<TestContext>,
        state: &mut VirtualListState<f32>,
        count: Option<usize>,
        input: Input<f32>,
        row: impl Fn(usize) -> (Id, f32),
    ) -> (Option<Rect<f32>>, Rect<f32>) {
        let thumb_id = frame.build(
            &mut TestContext,
            FrameInfo::new(Size::new(80.0, 50.0)),
            Duration::ZERO,
            input,
            |ui: Ui<'_, TestContext>| {
                let ui = ui.id(list_id());
                let thumb_id = ui.current_id().child("scroll thumb");
                ui.build(
                    VirtualList::new(state)
                        .count(count)
                        .track(|_: Ui<'_, TestContext>| {})
                        .thumb(|_: Ui<'_, TestContext>, _: bool| {})
                        .id(|index| row(index).0)
                        .item(|ui: Ui<'_, TestContext>, index| {
                            ui.layout(blit_layout::single::new().fixed(79.0, row(index).1));
                        }),
                );
                thumb_id
            },
        );
        frame.layout(&mut TestContext);
        (
            state
                .sample
                .get(state.visible.start)
                .and_then(|row| frame.geometry(row_id(row.id))),
            frame.geometry(thumb_id).unwrap(),
        )
    }

    fn list_id() -> Id {
        Id::new("test virtual list")
    }

    fn row_id(key: Id) -> Id {
        list_id().child(key)
    }
}
