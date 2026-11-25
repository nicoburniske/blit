use std::ops::Range;

use blit::{Axis, Constraints, Id, Key, Layout, LayoutCx, Point, Rect, Scalar, Scope, Size, Ui, state};

use super::{Scrollbar, ScrollbarInput, THUMB, TRACK, ThumbProgress, content_input, content_motion, scrollbar_input};
use crate::Context;

blit::builder! {
    /// use for large lists with variable or unknown row extents
    /// measures a bounded viewport window and estimates remaining extents
    /// rows must have positive finite extents along the axis
    /// an absent count scrolls indefinitely with a thumb covering the explored range
    pub struct VirtualList<'a, T: Scalar> {
        new(state: &'a mut VirtualListState<T>),
        axis: Axis = Axis::Vertical,
        #[into]
        count: Option<usize> = None,
        scroll_speed: f32 = 1.0,
        inertia_friction: f32 = 2.5,
        minimum_thumb_extent: T = T::from_f32(1.0),
        /// scrollbars cover the content instead of taking space beside it
        floating: bool = false,
    }
}

impl<'a, T: Scalar> VirtualList<'a, T> {
    /// starts the list in `ui`, whose rows then come from [`Rows::next`]
    pub fn begin<C, S, R, H>(self, mut ui: Ui<'a, C, S>, track: R, thumb: H) -> Rows<'a, C, R, H>
    where
        C: Context<Scalar = T>,
        S: state::NoLayout,
        R: FnOnce(Ui<'_, C>),
        H: FnOnce(Ui<'_, C>, bool),
    {
        let Self {
            state,
            axis,
            count,
            scroll_speed,
            inertia_friction,
            minimum_thumb_extent,
            floating,
        } = self;
        let viewport = ui.area();
        let size = viewport.map_or(ui.screen().size(), |area| area.size());
        let extent = axis.extent(size);
        let overscan = extent.repeat(VIEWPORTS);
        let row = state.sample.get(state.visible.start);
        let motion = content_input(&mut ui, axis);
        // the scrollbar and the rows key into scopes of their own, so no row key meets a part
        let (parts, rows) = (ui.scope(), ui.scope());
        let (thumb_active, scrollbar) = scrollbar_input(&mut state.inertia, &mut ui, parts, axis, extent);
        let movement = match scrollbar {
            None => content_motion(
                &mut state.inertia,
                &mut ui,
                axis,
                scroll_speed,
                inertia_friction,
                motion,
            ),
            Some(_) => 0.0,
        };
        if viewport.is_none() {
            ui.request_frame();
        }
        // treat large offsets into an unmeasured window as movement so the existing seek logic estimates the row
        if state.sample.is_empty() && state.within.to_f32().abs() > overscan.to_f32() {
            state.scroll_delta = state.within.endpoint(state.scroll_delta);
            state.within = T::ZERO;
        }
        let delta = {
            let delta: T = state.inertia.pixel_delta(movement);
            let delta = delta.endpoint(std::mem::take(&mut state.scroll_delta));
            if delta.to_f32().abs() > overscan.to_f32() && state.average == 0.0 {
                // wait for the initial extents before estimating a large jump
                state.scroll_delta = delta;
                ui.request_frame();
                T::ZERO
            } else {
                delta
            }
        };
        let current = state.first as f64
            + row.map_or(0.0, |row| {
                (state.within.to_f32() / row.extent.to_f32()).clamp(0.0, 1.0) as f64
            });
        let seek = match scrollbar {
            None | Some(ScrollbarInput::Idle | ScrollbarInput::Thumb(0.0)) => {
                if delta.to_f32().abs() > overscan.to_f32()
                    && state.average > 0.0
                    && !row.zip(state.sample.last()).is_some_and(|(row, last)| {
                        let offset = row.top.endpoint(state.within).endpoint(delta);
                        offset >= T::ZERO && offset <= last.top.endpoint(last.extent)
                    })
                {
                    Some((current + delta.to_f32() as f64 / state.average as f64).max(0.0))
                } else {
                    None
                }
            }
            Some(ScrollbarInput::Thumb(value) | ScrollbarInput::Track(value)) => {
                state.inertia.remainder = 0.0;
                let offset = row.map_or(T::ZERO, |row| row.top.endpoint(state.within));
                let (position, visible) = metrics(state, count, extent, offset);
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
            state.within = T::ZERO;
        }
        state.first = state.first.min(count.unwrap_or(usize::MAX).saturating_sub(1));
        let anchor = Anchor {
            index: state.first,
            within: state.within,
            delta,
            fraction: seek.map(|position| (position - state.first as f64).clamp(0.0, 1.0) as f32),
        };
        // the track sized the gutter last frame
        let cross = axis.other();
        let gutter = match floating {
            true => T::ZERO,
            false => {
                let track = ui.resolve(parts, TRACK);
                ui.node(track).area().map_or(T::ZERO, |area| cross.extent(area.size()))
            }
        };
        let mut bounds = Constraints {
            min: Size::ZERO,
            max: Size::uniform(T::UNBOUNDED),
        };
        let width = (cross.extent(size) - gutter).max(T::ZERO);
        cross.set_extent(&mut bounds.min, width);
        cross.set_extent(&mut bounds.max, width);
        let content = ui.clip(T::ZERO).layout(VirtualLayout {
            axis,
            anchor,
            count,
            size,
            row_bounds: bounds,
            scrollbar: Scrollbar {
                minimum: minimum_thumb_extent,
                floating,
            },
        });
        state.sample.clear();
        let list = List {
            state,
            content,
            parts,
            rows,
            axis,
            count,
            anchor,
            size,
            bounds,
            viewport,
            movement,
            direction,
            thumb_active,
            window: Window {
                first: anchor.index,
                end: anchor.index,
                within: anchor.within,
                ..Window::default()
            },
            pending: None,
            phase: Phase::Down,
            track: Some(track),
            thumb: Some(thumb),
        };
        Rows {
            list,
            keys: |index| index,
        }
    }
}

/// the rows of a [`VirtualList`], measured as they are built
///
/// rows are keyed by their index unless [`Rows::keys`] names them.
/// the scrollbar is built once the rows run out or this is dropped
pub struct Rows<'a, C, R, H, K = fn(usize) -> usize>
where
    C: Context,
    R: FnOnce(Ui<'_, C>),
    H: FnOnce(Ui<'_, C>, bool),
{
    list: List<'a, C, R, H>,
    keys: K,
}

impl<'a, C, R, H, K> Rows<'a, C, R, H, K>
where
    C: Context,
    R: FnOnce(Ui<'_, C>),
    H: FnOnce(Ui<'_, C>, bool),
{
    /// the next row to build and its index, until the viewport is covered
    #[allow(clippy::should_implement_trait)]
    pub fn next<O: Key>(&mut self) -> Option<(usize, Ui<'_, C, state::Child<ListItem>>)>
    where
        K: FnMut(usize) -> O,
    {
        self.list.next(&mut self.keys)
    }

    /// names the row at each index, so a row keeps its state as rows before it come and go
    pub fn keys<F, O>(self, keys: F) -> Rows<'a, C, R, H, F>
    where
        F: FnMut(usize) -> O,
        O: Key,
    {
        Rows { list: self.list, keys }
    }
}

// everything but the keys, which builds the scrollbar when dropped and so cannot be taken apart
struct List<'a, C, R, H>
where
    C: Context,
    R: FnOnce(Ui<'_, C>),
    H: FnOnce(Ui<'_, C>, bool),
{
    state: &'a mut VirtualListState<C::Scalar>,
    content: Ui<'a, C, state::Open<VirtualLayout<C::Scalar>>>,
    parts: Scope,
    rows: Scope,
    axis: Axis,
    count: Option<usize>,
    anchor: Anchor<C::Scalar>,
    size: Size<C::Scalar>,
    bounds: Constraints<C::Scalar>,
    viewport: Option<Rect<C::Scalar>>,
    movement: f32,
    direction: f64,
    thumb_active: bool,
    window: Window<C::Scalar>,
    // the row handed out last, measured on the next call
    pending: Option<(usize, Id)>,
    phase: Phase,
    // taken once the rows run out
    track: Option<R>,
    thumb: Option<H>,
}

impl<C, R, H> List<'_, C, R, H>
where
    C: Context,
    R: FnOnce(Ui<'_, C>),
    H: FnOnce(Ui<'_, C>, bool),
{
    fn next<K: Key>(&mut self, key: impl FnOnce(usize) -> K) -> Option<(usize, Ui<'_, C, state::Child<ListItem>>)> {
        if self.phase == Phase::Done {
            return None;
        }
        self.measure();
        let window = &self.window;
        let limit = self.count.unwrap_or(usize::MAX);
        let extent = self.axis.extent(self.size);
        let overscan = extent.repeat(VIEWPORTS);
        let index = if self.phase == Phase::Down
            && window.end < limit
            && (window.end == self.anchor.index || window.after_anchor < window.within.endpoint(overscan).max(extent))
        {
            window.end
        } else if window.first > 0
            && (-window.before_anchor > window.within
                || (window.end == limit && window.after_anchor.endpoint(window.before_anchor) < overscan))
        {
            self.phase = Phase::Up;
            window.first - 1
        } else {
            self.finish();
            return None;
        };
        let row = self
            .content
            .child_in(self.rows, key(index))
            .item(ListItem(Kind::Row(index)));
        self.pending = Some((index, row.id()));
        Some((index, row))
    }

    // the row built since the last call joins the window
    fn measure(&mut self) {
        let Some((index, node)) = self.pending.take() else {
            return;
        };
        let extent = self.axis.extent(self.content.layout_child(node, self.bounds));
        assert!(
            extent.is_finite() && extent > C::Scalar::ZERO,
            "virtual list rows need a positive extent"
        );
        let top = self.window.push(self.anchor, index, extent);
        self.state.sample.push(Row { top, extent });
    }

    fn finish(&mut self) {
        self.measure();
        self.phase = Phase::Done;
        let (Some(track), Some(thumb)) = (self.track.take(), self.thumb.take()) else {
            return;
        };
        let state = &mut *self.state;
        let window = &self.window;
        let anchor = self.anchor;
        let count = self.count;
        let extent = self.axis.extent(self.size);
        let first = window.first;
        if first < anchor.index {
            state.sample.reverse();
            state.sample[anchor.index - first..].reverse();
            for row in &mut state.sample {
                row.top = row.top.endpoint(window.before_anchor);
            }
        }
        let content_extent = window.after_anchor.endpoint(window.before_anchor);
        let offset = window.offset(extent, count);
        if (first == 0 && offset == C::Scalar::ZERO && self.movement < 0.0)
            || (count == Some(window.end)
                && offset == (content_extent - extent).max(C::Scalar::ZERO)
                && self.movement > 0.0)
        {
            state.inertia.velocity = 0.0;
            state.inertia.remainder = 0.0;
        }
        state.visible = state
            .sample
            .partition_point(|row| row.top.endpoint(row.extent) <= offset)
            ..state.sample.partition_point(|row| row.top < offset.endpoint(extent));
        if let Some(row) = state.sample.get(state.visible.start) {
            state.first = first + state.visible.start;
            state.within = offset - row.top;
        } else {
            state.first = first;
            state.within = C::Scalar::ZERO;
        }
        if !state.sample.is_empty() {
            state.average = content_extent.to_f32() / state.sample.len() as f32;
            if state.scrollbar.scale == 0.0 {
                state.scrollbar.scale = state.average;
            }
        }
        let (position, visible) = metrics(state, count, extent, offset);
        let progress = {
            let scrollbar = &mut state.scrollbar;
            if position == 0.0 {
                scrollbar.position = 0.0;
            } else if self.direction > 0.0 || scrollbar.position == 0.0 {
                scrollbar.position = scrollbar.position.max(position);
            } else if self.direction < 0.0 {
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
        // built after the rows so they draw and take input above them
        if progress.visible_fraction < 1.0 {
            self.content
                .child_in(self.parts, TRACK)
                .item(ListItem(Kind::Track(progress)))
                .build(track);
            let thumb_active = self.thumb_active;
            self.content
                .child_in(self.parts, THUMB)
                .item(ListItem(Kind::Thumb))
                .build(|ui: Ui<'_, C>| thumb(ui, thumb_active));
        }
        if self.viewport.is_none() {
            // preserve the anchor until the viewport is known
            state.first = anchor.index;
            state.within = anchor.within.endpoint(anchor.delta);
            state.sample.clear();
            state.visible = 0..0;
        }
    }
}

impl<C, R, H> Drop for List<'_, C, R, H>
where
    C: Context,
    R: FnOnce(Ui<'_, C>),
    H: FnOnce(Ui<'_, C>, bool),
{
    fn drop(&mut self) {
        self.finish();
    }
}

#[derive(Default)]
pub struct VirtualListState<T> {
    inertia: super::Inertia,

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
        self.inertia.stop();
    }

    /// scrolls to a list index and pixel offset
    /// - nearby offsets use exact row measurements
    /// - large offscreen offsets use estimated row extents
    pub fn scroll_to(&mut self, index: usize, offset: T) {
        self.first = index;
        self.scrollbar.position = index as f64;
        self.within = offset;
        self.sample.clear();
        self.visible = 0..0;
        self.scroll_delta = T::ZERO;
        self.inertia.stop();
    }

    /// returns the row index and pixel offset
    pub fn position(&self) -> (usize, T) {
        (self.first, self.within)
    }
}

/// a child's role in a [`VirtualList`]
#[derive(Clone, Copy)]
pub struct ListItem(Kind);

impl Default for ListItem {
    fn default() -> Self {
        Self(Kind::Row(0))
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Row(usize),
    Track(ThumbProgress),
    Thumb,
}

const VIEWPORTS: usize = 2;

// rows extend forward from the anchor, then backward above it
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Down,
    Up,
    Done,
}

struct Row<T> {
    top: T,
    extent: T,
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
    fn push(&mut self, anchor: Anchor<T>, index: usize, extent: T) -> T {
        self.first = self.first.min(index);
        self.end = self.end.max(index + 1);
        if index == anchor.index {
            self.within = match anchor.fraction {
                Some(fraction) => T::from_f32(extent.to_f32() * fraction),
                None => anchor.within.min(extent).endpoint(anchor.delta),
            };
        }
        if index < anchor.index {
            self.before_anchor = self.before_anchor.endpoint(extent);
            -self.before_anchor
        } else {
            let top = self.after_anchor;
            self.after_anchor = self.after_anchor.endpoint(extent);
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

struct VirtualLayout<T> {
    axis: Axis,
    anchor: Anchor<T>,
    count: Option<usize>,
    size: Size<T>,
    row_bounds: Constraints<T>,
    scrollbar: Scrollbar<T>,
}

impl<C: Context<Scalar = T>, T: Scalar> Layout<C> for VirtualLayout<T> {
    type Item = ListItem;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints<T>) -> Size<T> {
        let axis = self.axis;
        let cross = axis.other();
        // rows keep the bounds they were measured with until the viewport geometry catches up
        let mut window = Window {
            first: self.anchor.index,
            end: self.anchor.index,
            ..Window::default()
        };
        let mut track = None;
        let mut thumb = None;
        for child in cx.children() {
            match cx.item(child).0 {
                Kind::Row(index) => {
                    let extent = axis.extent(cx.layout_child(child, self.row_bounds));
                    window.push(self.anchor, index, extent);
                }
                Kind::Track(progress) => track = Some((child, progress)),
                Kind::Thumb => thumb = Some(child),
            }
        }
        let content_extent = window.after_anchor.endpoint(window.before_anchor);
        let mut size = Size::ZERO;
        axis.set_extent(
            &mut size,
            content_extent.endpoint(if self.count == Some(window.end) {
                T::ZERO
            } else {
                axis.extent(self.size)
            }),
        );
        let width = cross.extent(constraints.max);
        cross.set_extent(
            &mut size,
            if width.is_finite() {
                width
            } else {
                cross.extent(self.size)
            },
        );
        let size = constraints.constrain(size);
        let offset = window.offset(axis.extent(size), self.count);
        let mut positions = Window::default();
        for child in cx.children() {
            if let Kind::Row(index) = cx.item(child).0 {
                let extent = axis.extent(cx.size(child));
                let top = positions
                    .push(self.anchor, index, extent)
                    .endpoint(window.before_anchor)
                    - offset;
                let position = match axis {
                    Axis::Horizontal => Point::new(top, T::ZERO),
                    Axis::Vertical => Point::new(T::ZERO, top),
                };
                cx.set_child_position(child, position);
            }
        }
        if size != self.size
            || offset < T::ZERO
            || content_extent - offset < axis.extent(size) && self.count != Some(window.end)
        {
            cx.request_frame();
        }
        if let Some(((track, progress), thumb)) = track.zip(thumb) {
            // the track's own size sets the scrollbar thickness
            let thickness = cross
                .extent(cx.layout_child(track, Constraints::loose(constraints.max)))
                .min(cross.extent(size));
            self.scrollbar.layout(cx, axis, size, progress, thickness, track, thumb);
        }
        size
    }
}

fn metrics<T: Scalar>(state: &VirtualListState<T>, count: Option<usize>, extent: T, offset: T) -> (f64, f64) {
    let position_at = |index, offset: T| {
        state.sample.get(index).map_or(state.first as f64, |row: &Row<T>| {
            state.first as f64 + index as f64 - state.visible.start as f64
                + ((offset - row.top).to_f32() / row.extent.to_f32()).clamp(0.0, 1.0) as f64
        })
    };
    let position = position_at(state.visible.start, offset);
    let visible = if count.is_some() {
        position_at(state.visible.end.saturating_sub(1), offset.endpoint(extent)) - position
    } else {
        let scale = if state.scrollbar.scale > 0.0 {
            state.scrollbar.scale
        } else {
            state.average
        };
        extent.to_f32() as f64 / scale as f64
    };
    (position, visible)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{
        FrameInfo, Kernel, Scope,
        input::{Input, TouchPhase},
    };

    use super::*;
    use crate::test::{TestContext, pointer};

    #[test]
    fn shrinking_anchor_keeps_the_window_bounded() {
        for count in [Some(1_000_000), None] {
            let mut kernel = Kernel::default();
            let mut state = VirtualListState::default();
            let tall = |index| (index as u64, 10_000.0);
            settle_frame(&mut kernel, &mut state, count, tall);
            state.scroll_by(5_000.0);
            settle_frame(&mut kernel, &mut state, count, tall);
            settle_frame(&mut kernel, &mut state, count, |index| (index as u64, 1.0));
            assert!(state.sample.len() <= 101);
            assert_eq!(state.position().0, 1);
            assert_eq!(
                kernel
                    .find(Scope::ROOT, 1usize)
                    .and_then(|id| kernel.area(id))
                    .unwrap()
                    .y,
                0.0
            );
        }
    }

    #[test]
    fn queued_movement_and_position_preserve_the_row_offset() {
        for count in [Some(1_000_000), None] {
            let mut kernel = Kernel::default();
            let mut state = VirtualListState::default();
            state.scroll_by(5_000_000.0);
            settle_frame(&mut kernel, &mut state, count, |index| (index as u64, 10.0));
            assert_eq!(state.position().0, 500_000);
            state.scroll_by(1.0);
            state.scroll_by(5.0);
            render_frame(&mut kernel, &mut state, count, Input::None, |index| {
                (index as u64, 10.0)
            });
            let (index, offset) = state.position();
            assert_eq!(offset, 5.0);
            state.scroll_to(index + 1, offset);
            let (area, _) = render_frame(
                &mut kernel,
                &mut state,
                count.map(|count| count + 1),
                Input::None,
                |index| {
                    let id = if index == 0 {
                        u32::MAX as u64
                    } else {
                        (index - 1) as u64
                    };
                    (id, 10.0)
                },
            );
            assert_eq!(state.position(), (500_001, 5.0));
            assert_eq!(area.unwrap().y, -5.0);
        }
    }

    #[test]
    fn thumb_end_seeks_the_finite_or_explored_boundary() {
        for id in [None, Some(1)] {
            for (count, extents) in [
                (Some(1), &[1_000.0][..]),
                (Some(3), &[10.0, 10.0, 1_000.0]),
                (Some(20), &[10.0; 20]),
                (Some(3), &[10.0, 500.0, 10.0]),
                (Some(2), &[10.0, 10.0]),
                (None, &[10.0]),
            ] {
                let mut kernel = Kernel::default();
                let mut state = VirtualListState::default();
                let row = |index: usize| (index as u64, extents[index % extents.len()]);
                settle_frame(&mut kernel, &mut state, count, row);
                for phase in [TouchPhase::Down, TouchPhase::Up] {
                    render_frame(&mut kernel, &mut state, count, pointer(id, phase, 79.5, 49.999), row);
                }
                if kernel.has_pending_redraw() {
                    settle_frame(&mut kernel, &mut state, count, row);
                }
                if let Some(count) = count {
                    let area = kernel
                        .find(Scope::ROOT, count - 1)
                        .and_then(|id| kernel.area(id))
                        .unwrap();
                    assert_eq!(area.y + area.height, extents.iter().sum::<f32>().min(50.0));
                } else {
                    assert_eq!(state.position().0, 5);
                    assert_eq!(state.within, 0.0);
                }
            }
        }
    }

    #[test]
    fn variable_rows_keep_the_thumb_moving_with_the_scroll() {
        let mut kernel = Kernel::default();
        let mut state = VirtualListState::default();
        let row = |index| (index as u64, [1.0, 1024.0, 16.0, 512.0][index % 4]);
        let mut previous = settle_frame(&mut kernel, &mut state, None, row).1.unwrap();
        for delta in [16.0, -16.0] {
            for _ in 0..400 {
                state.scroll_by(delta);
                let thumb = render_frame(&mut kernel, &mut state, None, Input::None, row).1.unwrap();
                assert!((thumb.y - previous.y) * delta >= -0.0001);
                if delta > 0.0 {
                    assert!(thumb.height <= previous.height);
                }
                assert!(thumb.y >= 0.0 && thumb.y + thumb.height <= 50.00001);
                let idle = render_frame(&mut kernel, &mut state, None, Input::None, row).1.unwrap();
                assert_eq!(idle, thumb);
                assert!(!kernel.has_pending_redraw());
                previous = thumb;
            }
        }
        assert_eq!(previous.y, 0.0);
    }

    #[test]
    fn scrolling_up_preserves_the_existing_row_position() {
        for count in [Some(1_000), None] {
            for (before, after) in [(1024.0, 16.0), (16.0, 1024.0)] {
                let mut kernel = Kernel::default();
                let mut state = VirtualListState::default();
                let row = |index| (index as u64, if index < 20 { before } else { after });
                state.scroll_to(20, 0.0);
                settle_frame(&mut kernel, &mut state, count, row);
                let key = 20usize as u64;
                let id = kernel.find(Scope::ROOT, key).unwrap();
                let y = kernel.area(id).unwrap().y;
                state.scroll_by(-20.0);
                render_frame(&mut kernel, &mut state, count, Input::None, row);
                assert_eq!(kernel.area(id).unwrap().y, y + 20.0);
                settle_frame(&mut kernel, &mut state, count, row);
                assert_eq!(kernel.area(id).unwrap().y, y + 20.0);
                state.scroll_by(40.0);
                render_frame(&mut kernel, &mut state, count, Input::None, row);
                assert_eq!(kernel.area(id).unwrap().y, y - 20.0);
            }
        }
    }

    #[test]
    fn horizontal_rows_flow_along_x() {
        let mut kernel = Kernel::default();
        let mut state = VirtualListState::default();
        for _ in 0..2 {
            kernel.build(
                &mut TestContext,
                FrameInfo::new(Size::new(50.0, 20.0)),
                Duration::ZERO,
                Input::None,
                |ui: Ui<'_, TestContext>| {
                    let mut list =
                        VirtualList::new(&mut state)
                            .axis(Axis::Horizontal)
                            .count(10)
                            .begin(ui, |_| {}, |_, _| {});
                    while let Some((index, row)) = list.next() {
                        row.constrain(Constraints::tight(Size::new(10.0, 20.0)))
                            .layout(())
                            .child_in(Scope::ROOT, index)
                            .insert(());
                    }
                },
            );
            kernel.layout(&mut TestContext);
        }
        assert_eq!(
            kernel.find(Scope::ROOT, 2usize).and_then(|id| kernel.area(id)),
            Some(Rect::new(20.0, 0.0, 10.0, 20.0))
        );
    }

    fn settle_frame(
        kernel: &mut Kernel<TestContext>,
        state: &mut VirtualListState<f32>,
        count: Option<usize>,
        row: impl Fn(usize) -> (u64, f32),
    ) -> (Option<Rect<f32>>, Option<Rect<f32>>) {
        for _ in 0..8 {
            let result = render_frame(kernel, state, count, Input::None, &row);
            if !kernel.has_pending_redraw() {
                return result;
            }
        }
        panic!("virtual list did not converge");
    }

    fn render_frame(
        kernel: &mut Kernel<TestContext>,
        state: &mut VirtualListState<f32>,
        count: Option<usize>,
        input: Input<f32>,
        row: impl Fn(usize) -> (u64, f32),
    ) -> (Option<Rect<f32>>, Option<Rect<f32>>) {
        let mut thumb = None;
        kernel.build(
            &mut TestContext,
            FrameInfo::new(Size::new(80.0, 50.0)),
            Duration::ZERO,
            input,
            |ui: Ui<'_, TestContext>| {
                // a track at least one unit thick on the right edge
                let track = |ui: Ui<'_, TestContext>| {
                    ui.constrain(Constraints {
                        min: Size::uniform(1.0),
                        max: Size::uniform(f32::INFINITY),
                    });
                };
                let mut list = VirtualList::new(&mut *state)
                    .count(count)
                    .begin(ui, track, |ui: Ui<'_, TestContext>, _| thumb = Some(ui.id()))
                    .keys(|index| row(index).0);
                while let Some((index, ui)) = list.next() {
                    let (id, extent) = row(index);
                    // a marker in the root scope covers the row so the frame can find it
                    ui.constrain(Constraints::tight(Size::new(79.0, extent)))
                        .layout(())
                        .child_in(Scope::ROOT, id)
                        .insert(());
                }
            },
        );
        kernel.layout(&mut TestContext);
        (
            kernel
                .find(Scope::ROOT, row(state.position().0).0)
                .and_then(|id| kernel.area(id)),
            thumb.and_then(|id| kernel.area(id)),
        )
    }
}
