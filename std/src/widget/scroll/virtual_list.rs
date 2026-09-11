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
        let mut table = state.table.borrow_mut();
        state.scroll.offset = table.offset;
        state.scroll.content_extent = table.total;
        let previous = state.scroll.offset;
        let elapsed = state.scroll.last_frame.map_or(0.0, |previous| {
            ui.time().saturating_sub(previous).as_secs_f32()
        });
        let (thumb_active, _) = update::<_, S>(&mut state.scroll, &mut ui, Axis::Vertical, config);
        let index = table
            .rows
            .partition_point(|row| row.top + row.height <= state.scroll.offset);
        let mut target = table
            .rows
            .get(index)
            .map(|row| (index, state.scroll.offset - row.top));
        let mut full = false;
        if table.rows.len() != rows.len() || state.dirty {
            let anchor = target.map(|(index, within)| (table.rows[index].id, within));
            table.rows.clear();
            target = None;
            let mut top = 0.0;
            for value in rows {
                let id = key(value);
                let height = table.heights.get(&id).copied();
                full |= height.is_none();
                let height = height.unwrap_or(0.0);
                if let Some((anchor, within)) = anchor
                    && anchor == id
                {
                    target = Some((table.rows.len(), within));
                }
                table.rows.push(Row { id, top, height });
                top += height;
            }
            table.total = top;
            state.dirty = false;
            ui.request_frame();
        }
        let reveal = state.reveal.take();
        if let Some(id) = reveal {
            target = table
                .rows
                .iter()
                .position(|row| row.id == id)
                .map(|index| (index, 0.0));
        }
        if let Some((index, within)) = target {
            state.scroll.offset = table.rows[index].top + within.min(table.rows[index].height);
        }
        if reveal.is_some() {
            state.scroll.velocity = 0.0;
            state.scroll.tracking = false;
        }
        let mut visible = 0..0;
        let mut pointer_row = None;
        if !full && let Some(viewport) = viewport {
            state.scroll.content_extent = table.total;
            state.scroll.viewport_extent = viewport.height;
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
                    table
                        .rows
                        .partition_point(|row| row.top + row.height <= y)
                        .min(rows.len() - 1),
                );
            }
            let first = table
                .rows
                .partition_point(|row| row.top + row.height <= state.scroll.offset);
            let end = table
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
        let target = if full {
            visible = 0..rows.len();
            ui.request_frame();
            target
        } else {
            table
                .rows
                .get(state.visible.start)
                .map(|row| (state.visible.start, state.scroll.offset - row.top))
        };
        let offset = state.scroll.offset;
        table.offset = offset;
        drop(table);
        let table = Rc::clone(&state.table);
        let first = visible.start;
        build_scroll(
            ui,
            state.scroll.id,
            MeasuredScrollLayout {
                scroll: ScrollLayout {
                    axis: Axis::Vertical,
                    offset,
                    scrollbar_thickness: config.scrollbar_thickness,
                    minimum_thumb_extent: config.minimum_thumb_extent,
                },
                table: Rc::clone(&table),
            },
            clip,
            move |ui: Ui<'_, R>| {
                let mut list = ui.layout(MeasuredLayout {
                    first,
                    target,
                    table,
                });
                for index in visible {
                    item(list.child(()), &rows[index]);
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
    table: Rc<RefCell<RowTable>>,
    screen: Option<Size>,
    dirty: bool,
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
        self.table.borrow_mut().heights.remove(&id);
        self.dirty = true;
    }
    pub fn invalidate_all(&mut self) {
        self.table.borrow_mut().heights.clear();
        self.dirty = true;
    }
    pub fn is_visible(&self, id: WidgetId) -> bool {
        self.table.borrow().rows[self.visible.clone()]
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
struct RowTable {
    rows: Vec<Row>,
    heights: HashMap<WidgetId, f32>,
    total: f32,
    offset: f32,
}

struct MeasuredScrollLayout {
    scroll: ScrollLayout,
    table: Rc<RefCell<RowTable>>,
}

impl<R: Platform> Layout<R> for MeasuredScrollLayout {
    type Item = super::ScrollItem;

    fn layout(&self, ui: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        self.scroll.layout_with_offset(ui, constraints, |maximum| {
            let mut table = self.table.borrow_mut();
            table.offset = table.offset.clamp(0.0, maximum);
            table.offset
        })
    }

    fn override_size(&self, _: &mut Self::Item, _: Option<f32>, _: Option<f32>) -> bool {
        false
    }
}

struct MeasuredLayout {
    first: usize,
    target: Option<(usize, f32)>,
    table: Rc<RefCell<RowTable>>,
}

impl<R: Platform> Layout<R> for MeasuredLayout {
    type Item = ();

    fn layout(&self, ui: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let mut table = self.table.borrow_mut();
        let mut changed = None;
        for (index, child) in ui.children().enumerate() {
            let size = ui.layout_child(
                child,
                Constraints {
                    min: Size::new(constraints.max.width, 0.0),
                    max: Size::new(constraints.max.width, f32::INFINITY),
                },
            );
            assert!(size.height.is_finite() && size.height >= 0.0);
            let row = &mut table.rows[self.first + index];
            if row.height != size.height {
                changed.get_or_insert(self.first + index);
                row.height = size.height;
            }
            let id = row.id;
            table.heights.insert(id, size.height);
        }
        if let Some(first) = changed {
            let mut top = table.rows[first].top;
            for row in &mut table.rows[first..] {
                row.top = top;
                top += row.height;
            }
            table.total = top;
            ui.request_frame();
        }
        if let Some((index, within)) = self.target {
            let row = &table.rows[index];
            table.offset = row.top + within.min(row.height);
        }
        for (index, child) in ui.children().enumerate() {
            ui.set_child_position(child, Point::new(0.0, table.rows[self.first + index].top));
        }
        constraints.constrain(Size::new(constraints.max.width, table.total))
    }

    fn override_size(&self, _: &mut Self::Item, _: Option<f32>, _: Option<f32>) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blit::{Frame, FrameInfo, Rect};

    struct TestPlatform;
    impl Platform for TestPlatform {
        fn begin(&mut self, _: FrameInfo) {}
        fn end(&mut self) {}
    }

    struct TestClip;
    impl Clip<TestPlatform> for TestClip {
        fn push(&self, _: &mut TestPlatform, _: Rect) {}
        fn pop(&self, _: &mut TestPlatform) {}
    }

    #[test]
    fn remeasured_rows_resolve_scroll_before_the_first_paint() {
        use super::super::{MeasuredState, VirtualList};
        use blit::{Sides, WidgetId};

        fn render(frame: &mut Frame<TestPlatform>, state: &mut MeasuredState, rows: &[(u32, f32)]) {
            frame.render(
                &mut TestPlatform,
                FrameInfo::new(Size::new(80.0, 50.0)),
                VirtualList::<_, _, _, NoScrollbar>::new(state, TestClip, rows)
                    .key(|row| WidgetId::new(row.0))
                    .build(|ui, row| {
                        ui.widget_id(WidgetId::new(row.0))
                            .layout(crate::layout::single::layout().padding(Sides::y(row.1 / 2.0)));
                    }),
            );
        }

        let mut frame = Frame::default();
        let mut state = MeasuredState::default();
        let mut rows: Vec<_> = (0..40u32).map(|id| (id, 10.0 + (id % 3) as f32)).collect();
        state.scroll_to(WidgetId::new(20u32));
        render(&mut frame, &mut state, &rows);
        assert_eq!(frame.geometry(WidgetId::new(20u32)).unwrap().y, 0.0);
        render(&mut frame, &mut state, &rows);

        rows.retain(|row| row.0 % 2 == 0);
        for row in &mut rows {
            row.1 *= 2.0;
        }
        state.invalidate_all();
        state.scroll_to(WidgetId::new(30u32));
        render(&mut frame, &mut state, &rows);
        let first = frame.geometry(WidgetId::new(30u32)).unwrap();
        assert_eq!(first.y, 0.0);
        render(&mut frame, &mut state, &rows);
        assert_eq!(frame.geometry(WidgetId::new(30u32)).unwrap(), first);

        rows.reverse();
        state.invalidate_all();
        render(&mut frame, &mut state, &rows);
        assert_eq!(frame.geometry(WidgetId::new(30u32)).unwrap().y, 0.0);
        render(&mut frame, &mut state, &rows);
        assert_eq!(frame.geometry(WidgetId::new(30u32)).unwrap().y, 0.0);

        state.invalidate_all();
        state.scroll_to(WidgetId::new(0u32));
        render(&mut frame, &mut state, &rows);
        let last = frame.geometry(WidgetId::new(0u32)).unwrap();
        assert_eq!(last.y + last.height, 50.0);
        render(&mut frame, &mut state, &rows);
        assert_eq!(frame.geometry(WidgetId::new(0u32)).unwrap(), last);

        state.scroll_to(WidgetId::new(30u32));
        render(&mut frame, &mut state, &rows);
        state.table.borrow_mut().offset += 5.0;
        render(&mut frame, &mut state, &rows);
        for row in &mut rows {
            if row.0 == 32 || row.0 == 30 {
                row.1 *= 2.0;
            }
        }
        render(&mut frame, &mut state, &rows);
        let anchor = frame.geometry(WidgetId::new(30u32)).unwrap();
        assert_eq!(anchor.y, -5.0);
        render(&mut frame, &mut state, &rows);
        assert_eq!(frame.geometry(WidgetId::new(30u32)).unwrap(), anchor);

        rows.clear();
        state.mark_dirty();
        render(&mut frame, &mut state, &rows);
        assert_eq!(state.table.borrow().offset, 0.0);
    }
}
