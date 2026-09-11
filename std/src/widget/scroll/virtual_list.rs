use super::{NoScrollbar, ScrollLayout, Scrollbar, State, build_scroll, update};
use blit::{
    Axis, Clip, Constraints, Content, Layout, LayoutCx, Platform, Point, Size, Ui, Widget, WidgetId,
};
use std::{cell::RefCell, collections::HashMap, marker::PhantomData, ops::Range, rc::Rc};

/// measures all rows initially then corrects visible heights as layout changes
pub struct VirtualList<'a, R, T, X, S = NoScrollbar, K = (), F = ()> {
    state: &'a mut MeasuredState,
    rows: &'a [T],
    clip: X,
    scrollbar: S,
    key: K,
    item: F,
    edge_scroll: bool,
    marker: PhantomData<fn() -> R>,
}

impl<'a, R: Platform, T, X, S: Default + Scrollbar> VirtualList<'a, R, T, X, S> {
    /// call MeasuredState::mark_dirty after changing the row sequence
    pub fn new(state: &'a mut MeasuredState, clip: X, rows: &'a [T]) -> Self {
        Self {
            state,
            rows,
            clip,
            scrollbar: S::default(),
            key: (),
            item: (),
            edge_scroll: false,
            marker: PhantomData,
        }
    }
}

impl<'a, R: Platform, T, X, S, K, F> VirtualList<'a, R, T, X, S, K, F> {
    /// identities must be unique and stable across filtering and reordering
    pub fn key<N>(self, key: N) -> VirtualList<'a, R, T, X, S, N, F>
    where
        N: FnMut(&T) -> WidgetId,
    {
        VirtualList {
            state: self.state,
            rows: self.rows,
            clip: self.clip,
            scrollbar: self.scrollbar,
            key,
            item: self.item,
            edge_scroll: self.edge_scroll,
            marker: PhantomData,
        }
    }

    pub fn build<N>(self, item: N) -> VirtualList<'a, R, T, X, S, K, N>
    where
        N: FnMut(Ui<'_, R>, &T),
    {
        VirtualList {
            state: self.state,
            rows: self.rows,
            clip: self.clip,
            scrollbar: self.scrollbar,
            key: self.key,
            item,
            edge_scroll: self.edge_scroll,
            marker: PhantomData,
        }
    }

    /// scrolls while the pointer is near or beyond the viewport edges
    pub fn edge_scroll(mut self, active: bool) -> Self {
        self.edge_scroll = active;
        self
    }
}

impl<R, T, X, S, K, F> Widget<R> for VirtualList<'_, R, T, X, S, K, F>
where
    R: Platform,
    X: Clip<R>,
    S: Scrollbar,
    S::Track: Content<R>,
    S::Thumb: Content<R>,
    K: FnMut(&T) -> WidgetId,
    F: FnMut(Ui<'_, R>, &T),
{
    type Response = VirtualListResponse;

    fn build(self, mut ui: Ui<'_, R>) -> Self::Response {
        let Self {
            state,
            rows,
            clip,
            scrollbar,
            mut key,
            mut item,
            edge_scroll,
            ..
        } = self;
        let config = scrollbar.config();
        let viewport = ui.geometry(state.scroll.id);
        let screen = ui.screen().size();
        if state.screen.replace(screen) != Some(screen) {
            ui.request_frame();
        }
        {
            let mut measured = state.measurements.borrow_mut();
            if measured.ready && !state.dirty {
                let anchor = state
                    .rows
                    .partition_point(|row| row.top + row.height <= state.scroll.offset);
                let within = state
                    .rows
                    .get(anchor)
                    .map(|row| state.scroll.offset - row.top);
                let mut changed = None;
                for (index, &height) in measured.heights.iter().enumerate() {
                    let index = measured.first + index;
                    let row = &mut state.rows[index];
                    if state.pending || row.height != height {
                        state.heights.insert(row.id, height);
                        row.height = height;
                        changed.get_or_insert(index);
                    }
                }
                if let Some(first) = changed {
                    let mut top = state.rows[first].top;
                    for row in &mut state.rows[first..] {
                        row.top = top;
                        top += row.height;
                    }
                    state.total = top;
                    if let Some(within) = within {
                        let row = &state.rows[anchor];
                        state.scroll.offset = row.top + within.min(row.height);
                    }
                    if let Some((id, within)) = state.anchor.take()
                        && let Some(row) = state.rows.iter().find(|row| row.id == id)
                    {
                        state.scroll.offset = row.top + within.min(row.height);
                    }
                }
                state.pending = false;
                state.scroll.content_extent = state.total;
            }
            measured.ready = false;
        }
        let previous = state.scroll.offset;
        let elapsed = state.scroll.last_frame.map_or(0.0, |previous| {
            ui.time().saturating_sub(previous).as_secs_f32()
        });
        let (thumb_active, _) = update(
            &mut state.scroll,
            &mut ui,
            Axis::Vertical,
            config,
            S::HAS_THUMB,
        );
        if state.rows.len() != rows.len() || state.dirty {
            if state.anchor.is_none() {
                let index = state
                    .rows
                    .partition_point(|row| row.top + row.height <= previous);
                state.anchor = state
                    .rows
                    .get(index)
                    .map(|row| (row.id, previous - row.top));
            }
            state.rows.clear();
            let mut top = 0.0;
            let mut complete = true;
            for value in rows {
                let id = key(value);
                let height = state.heights.get(&id).copied();
                complete &= height.is_some();
                let height = height.unwrap_or(0.0);
                state.rows.push(Row { id, top, height });
                top += height;
            }
            state.total = top;
            state.pending = !complete;
            state.dirty = false;
            if complete {
                if let Some((id, within)) = state.anchor.take()
                    && let Some(row) = state.rows.iter().find(|row| row.id == id)
                {
                    state.scroll.offset = row.top + within.min(row.height);
                }
            }
            ui.request_frame();
        }
        let mut visible = 0..0;
        let mut pointer_row = None;
        if !state.pending
            && let Some(viewport) = viewport
        {
            state.scroll.content_extent = state.total;
            state.scroll.viewport_extent = viewport.height;
            if let Some(id) = state.reveal.take()
                && let Some(row) = state.rows.iter().find(|row| row.id == id)
            {
                state.scroll.scroll_to(row.top);
            }
            state.scroll.offset = state
                .scroll
                .offset
                .clamp(0.0, state.scroll.maximum_offset());
            if edge_scroll {
                state.scroll.velocity = 0.0;
                if let Some(pointer) = ui.pointer_position() {
                    let y = pointer.y - viewport.y;
                    let edge = 40.0_f32.min(viewport.height / 2.0).max(1.0);
                    let speed = if y < edge {
                        -((edge - y) / edge).min(1.0)
                    } else {
                        ((y - viewport.height + edge) / edge).clamp(0.0, 1.0)
                    };
                    if speed != 0.0 {
                        state.scroll.scroll_by(speed * 600.0 * elapsed.min(0.05));
                        if (speed < 0.0 && state.scroll.offset > 0.0)
                            || (speed > 0.0 && state.scroll.offset < state.scroll.maximum_offset())
                        {
                            ui.request_frame();
                        }
                    }
                }
            }
            if let Some(pointer) = ui.pointer_position()
                && !rows.is_empty()
            {
                let y = state.scroll.offset + (pointer.y - viewport.y).clamp(0.0, viewport.height);
                pointer_row = Some(
                    state
                        .rows
                        .partition_point(|row| row.top + row.height <= y)
                        .min(rows.len() - 1),
                );
            }
            let first = state
                .rows
                .partition_point(|row| row.top + row.height <= state.scroll.offset);
            let end = state
                .rows
                .partition_point(|row| row.top < state.scroll.offset + viewport.height);
            state.visible = first..end;
            visible = first.saturating_sub(1)..(end + 1).min(rows.len());
        } else {
            state.visible = 0..0;
        }
        if state.scroll.offset != previous {
            ui.request_frame();
        }
        let full = state.pending;
        if full {
            visible = 0..rows.len();
            ui.request_frame();
        }
        let offset = state.scroll.offset;
        let total = state.total;
        let measurements = Rc::clone(&state.measurements);
        let first = visible.start;
        let anchor = state.visible.start;
        let geometry = &state.rows;
        build_scroll(
            ui,
            state.scroll.id,
            ScrollLayout {
                axis: Axis::Vertical,
                offset,
                scrollbar_thickness: config.scrollbar_thickness,
                minimum_thumb_extent: config.minimum_thumb_extent,
            },
            clip,
            move |ui: Ui<'_, R>| {
                let mut list = ui.layout(MeasuredLayout {
                    total,
                    first,
                    anchor,
                    measurements,
                });
                for index in visible {
                    let row = &geometry[index];
                    item(list.child((row.top, row.height)), &rows[index]);
                }
            },
            scrollbar,
            thumb_active,
        );
        VirtualListResponse { pointer_row }
    }
}

pub struct VirtualListResponse {
    /// row at the pointer y position clamped to the viewport edges
    /// none when the pointer or measurements are unavailable or the list is empty
    pub pointer_row: Option<usize>,
}

#[derive(Default)]
pub struct MeasuredState {
    scroll: State,
    rows: Vec<Row>,
    heights: HashMap<WidgetId, f32>,
    screen: Option<Size>,
    total: f32,
    dirty: bool,
    pending: bool,
    measurements: Rc<RefCell<Measurements>>,
    anchor: Option<(WidgetId, f32)>,
    reveal: Option<WidgetId>,
    visible: Range<usize>,
}

impl MeasuredState {
    pub fn id(&self) -> WidgetId {
        self.scroll.id
    }
    pub fn scroll_to(&mut self, id: WidgetId) {
        self.reveal = Some(id);
    }

    /// rebuilds the row index while retaining heights for existing keys
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn invalidate(&mut self, id: WidgetId) {
        self.pending = false;
        self.measurements.borrow_mut().ready = false;
        self.heights.remove(&id);
        self.dirty = true;
    }
    pub fn invalidate_all(&mut self) {
        self.pending = false;
        self.measurements.borrow_mut().ready = false;
        self.heights.clear();
        self.dirty = true;
    }
    pub fn is_visible(&self, id: WidgetId) -> bool {
        self.rows[self.visible.clone()]
            .iter()
            .any(|row| row.id == id)
    }
}

struct Row {
    id: WidgetId,
    top: f32,
    height: f32,
}
#[derive(Default)]
struct Measurements {
    first: usize,
    heights: Vec<f32>,
    ready: bool,
}

struct MeasuredLayout {
    total: f32,
    first: usize,
    anchor: usize,
    measurements: Rc<RefCell<Measurements>>,
}

impl<R: Platform> Layout<R> for MeasuredLayout {
    type Item = (f32, f32);

    fn layout(&self, ui: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let mut measured = self.measurements.borrow_mut();
        measured.first = self.first;
        measured.heights.clear();
        measured.ready = false;
        let mut delta = 0.0;
        let mut shift = 0.0;
        let mut changed = false;
        for (index, child) in ui.children().enumerate() {
            let (_, height) = *ui.item(child);
            let size = ui.layout_child(
                child,
                Constraints {
                    min: Size::new(constraints.max.width, 0.0),
                    max: Size::new(constraints.max.width, f32::INFINITY),
                },
            );
            assert!(size.height.is_finite() && size.height >= 0.0);
            measured.heights.push(size.height);
            changed |= size.height != height;
            delta += size.height - height;
            if self.first + index < self.anchor {
                shift += size.height - height;
            }
        }
        let mut top = ui.children().next().map_or(0.0, |child| ui.item(child).0) - shift;
        for (child, &height) in ui.children().zip(&measured.heights) {
            ui.set_child_position(child, Point::new(0.0, top));
            top += height;
        }
        measured.ready = true;
        if changed {
            ui.request_frame();
        }
        constraints.constrain(Size::new(
            constraints.max.width,
            (self.total + delta).max(0.0),
        ))
    }

    fn override_size(&self, _: &mut Self::Item, _: Option<f32>, _: Option<f32>) -> bool {
        false
    }
}
