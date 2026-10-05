use std::{cell::RefCell, collections::HashMap, ops::Range, rc::Rc};

use blit::{Axis, Clip, Constraints, Content, Context, Layout, LayoutCx, Point, Scalar, Size, Ui, WidgetId};

pub use super::Behavior;
use super::{ScrollLayout, build_scroll, update};

blit::builder! {
    #[derive(Clone, Copy, Debug)]
    pub struct Config<T: Scalar> {
        new(),
        edge_scroll: bool = false,
        behavior: Behavior<T> = Behavior::default(),
    }
}

/// measures all rows initially then corrects visible heights as layout changes
pub fn build<C: Context, R, X, K, F, T, H>(
    mut ui: Ui<'_, C>,
    state: &mut State<C::Scalar>,
    rows: &[R],
    clip: X,
    list: Config<C::Scalar>,
    scrollbar: impl FnOnce(bool) -> (Option<T>, Option<H>),
    mut widget_id: K,
    mut item: F,
) -> Response
where
    X: Clip<C>,
    T: Content<C>,
    H: Content<C>,
    K: FnMut(&R) -> WidgetId,
    F: FnMut(Ui<'_, C>, &R),
{
    let config = list.behavior;
    let edge_scroll = list.edge_scroll;
    let id = ui.current_widget_id();
    let viewport = ui.geometry(id);
    let screen = ui.screen().size();
    if state.screen.replace(screen) != Some(screen) {
        ui.request_frame();
    }
    let mut table = state.table.borrow_mut();
    state.scroll.offset = table.offset;
    state.scroll.content_extent = table.total;
    let previous = state.scroll.offset;
    let elapsed = state
        .scroll
        .last_frame
        .map_or(0.0, |previous| ui.time().saturating_sub(previous).as_secs_f32());
    let (thumb_active, _) = update(&mut state.scroll, &mut ui, Axis::Vertical, config);
    let index = table
        .rows
        .partition_point(|row| row.top.endpoint(row.height) <= state.scroll.offset);
    let mut target = table.rows.get(index).map(|row| (index, state.scroll.offset - row.top));
    let mut full = false;
    if table.rows.len() != rows.len() || state.dirty {
        let anchor = target.map(|(index, within)| (table.rows[index].id, within));
        table.rows.clear();
        target = None;
        let mut top = C::Scalar::ZERO;
        for value in rows {
            let id = widget_id(value);
            let height = table.heights.get(&id).copied();
            full |= height.is_none();
            let height = height.unwrap_or(C::Scalar::ZERO);
            if let Some((anchor, within)) = anchor
                && anchor == id
            {
                target = Some((table.rows.len(), within));
            }
            table.rows.push(Row { id, top, height });
            top = top.endpoint(height);
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
            .map(|index| (index, C::Scalar::ZERO));
    }
    if let Some((index, within)) = target {
        state.scroll.offset = table.rows[index].top.endpoint(within.min(table.rows[index].height));
    }
    if reveal.is_some() {
        state.scroll.content_extent = table.total;
        state.scroll.scroll_to(state.scroll.offset);
    }
    let mut visible = 0..0;
    let mut pointer_row = None;
    if !full && let Some(viewport) = viewport {
        state.scroll.content_extent = table.total;
        state.scroll.viewport_extent = viewport.height;
        state.scroll.offset = state.scroll.offset.clamp(C::Scalar::ZERO, state.scroll.maximum_offset());
        if edge_scroll {
            state.scroll.velocity = 0.0;
            state.scroll.tracking = false;
            if let Some(pointer) = ui.pointer_position() {
                let y = (pointer.y - viewport.y).to_f32();
                let edge = 40.0_f32.min(viewport.height.to_f32() / 2.0).max(1.0);
                let speed = if y < edge {
                    -((edge - y) / edge).min(1.0)
                } else {
                    ((y - viewport.height.to_f32() + edge) / edge).clamp(0.0, 1.0)
                };
                if speed != 0.0 {
                    super::move_by(&mut state.scroll, speed * 600.0 * elapsed.min(0.05));
                    if (speed < 0.0 && state.scroll.offset > C::Scalar::ZERO)
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
            let y = state
                .scroll
                .offset
                .endpoint((pointer.y - viewport.y).clamp(C::Scalar::ZERO, viewport.height));
            pointer_row = Some(
                table
                    .rows
                    .partition_point(|row| row.top.endpoint(row.height) <= y)
                    .min(rows.len() - 1),
            );
        }
        let first = table
            .rows
            .partition_point(|row| row.top.endpoint(row.height) <= state.scroll.offset);
        let end = table
            .rows
            .partition_point(|row| row.top < state.scroll.offset.endpoint(viewport.height));
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
    let (track, thumb) = scrollbar(thumb_active);
    build_scroll(
        ui,
        ScrollLayout {
            axis: Axis::Vertical,
            offset: {
                let table = Rc::clone(&table);
                move |maximum| {
                    let mut table = table.borrow_mut();
                    table.offset = table.offset.clamp(C::Scalar::ZERO, maximum);
                    table.offset
                }
            },
            scrollbar_thickness: config.scrollbar_thickness,
            minimum_thumb_extent: config.minimum_thumb_extent,
        },
        clip,
        move |ui: Ui<'_, C>| {
            let mut list = ui.layout(MeasuredLayout {
                first,
                target,
                table: Rc::clone(&table),
            });
            for index in visible {
                let id = table.borrow().rows[index].id;
                list.child().widget_id(id).build(|ui: Ui<'_, C>| item(ui, &rows[index]));
            }
        },
        track,
        thumb,
    );
    Response { pointer_row }
}

pub struct Response {
    /// row at the pointer y position clamped to the viewport edges
    /// none when the pointer or measurements are unavailable or the list is empty
    pub pointer_row: Option<usize>,
}

#[derive(Default)]
pub struct State<T> {
    scroll: super::State<T>,
    table: Rc<RefCell<RowTable<T>>>,
    screen: Option<Size<T>>,
    dirty: bool,
    reveal: Option<WidgetId>,
    visible: Range<usize>,
}

impl<T: Scalar> State<T> {
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

struct Row<T> {
    id: WidgetId,
    top: T,
    height: T,
}
#[derive(Default)]
struct RowTable<T> {
    rows: Vec<Row<T>>,
    heights: HashMap<WidgetId, T>,
    total: T,
    offset: T,
}

struct MeasuredLayout<T> {
    first: usize,
    target: Option<(usize, T)>,
    table: Rc<RefCell<RowTable<T>>>,
}

impl<C: Context<Scalar = T>, T: Scalar> Layout<C> for MeasuredLayout<T> {
    type Item = ();

    fn layout(&self, ui: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints<T>) -> Size<T> {
        let width = if constraints.max.width.is_finite() {
            constraints.max.width
        } else {
            ui.children().fold(constraints.min.width, |width, child| {
                width.max(
                    ui.layout_child(child, Constraints::loose(Size::uniform(T::UNBOUNDED)))
                        .width,
                )
            })
        };
        let bounds = Constraints {
            min: Size::new(width, T::ZERO),
            max: Size::new(width, T::UNBOUNDED),
        };
        let mut table = self.table.borrow_mut();
        let mut changed = None;
        for (index, child) in ui.children().enumerate() {
            let size = ui.layout_child(child, bounds);
            assert!(size.height.is_finite() && size.height >= T::ZERO);
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
                top = top.endpoint(row.height);
            }
            table.total = top;
            ui.request_frame();
        }
        if let Some((index, within)) = self.target {
            let row = &table.rows[index];
            table.offset = row.top.endpoint(within.min(row.height));
        }
        for (index, child) in ui.children().enumerate() {
            ui.set_child_position(child, Point::new(T::ZERO, table.rows[self.first + index].top));
        }
        constraints.constrain(Size::new(bounds.max.width, table.total))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, Input};

    use super::*;
    use crate::test::{TestClip, TestContext};

    #[test]
    fn remeasured_rows_resolve_scroll_before_the_first_paint() {
        use blit::WidgetId;
        use blit_layout::single;

        fn layout(frame: &mut Frame<TestContext>, state: &mut State<f32>, rows: &[(u32, f32)]) {
            let context = &mut TestContext;
            frame.build(
                context,
                FrameInfo::new(Size::new(80.0, 50.0)),
                Duration::ZERO,
                Input::None,
                |ui: Ui<'_, TestContext>| {
                    build(
                        ui,
                        state,
                        rows,
                        TestClip,
                        Config::new(),
                        |_| (None::<()>, None::<()>),
                        |row| WidgetId::new(row.0),
                        |ui, row| {
                            ui.layout(single::new().fixed(80.0, row.1));
                        },
                    )
                },
            );
            frame.layout(context);
        }

        let mut frame = Frame::default();
        let mut state = State::default();
        let mut rows: Vec<_> = (0..40u32).map(|id| (id, 10.0 + (id % 3) as f32)).collect();
        state.scroll_to(WidgetId::new(20u32));
        layout(&mut frame, &mut state, &rows);
        assert_eq!(frame.geometry(WidgetId::new(20u32)).unwrap().y, 0.0);
        layout(&mut frame, &mut state, &rows);

        rows.retain(|row| row.0 % 2 == 0);
        for row in &mut rows {
            row.1 *= 2.0;
        }
        state.invalidate_all();
        state.scroll_to(WidgetId::new(30u32));
        layout(&mut frame, &mut state, &rows);
        let first = frame.geometry(WidgetId::new(30u32)).unwrap();
        assert_eq!(first.y, 0.0);
        layout(&mut frame, &mut state, &rows);
        assert_eq!(frame.geometry(WidgetId::new(30u32)).unwrap(), first);

        rows.reverse();
        state.invalidate_all();
        layout(&mut frame, &mut state, &rows);
        assert_eq!(frame.geometry(WidgetId::new(30u32)).unwrap().y, 0.0);
        layout(&mut frame, &mut state, &rows);
        assert_eq!(frame.geometry(WidgetId::new(30u32)).unwrap().y, 0.0);

        state.invalidate_all();
        state.scroll_to(WidgetId::new(0u32));
        layout(&mut frame, &mut state, &rows);
        let last = frame.geometry(WidgetId::new(0u32)).unwrap();
        assert_eq!(last.y + last.height, 50.0);
        layout(&mut frame, &mut state, &rows);
        assert_eq!(frame.geometry(WidgetId::new(0u32)).unwrap(), last);

        state.scroll_to(WidgetId::new(30u32));
        layout(&mut frame, &mut state, &rows);
        state.table.borrow_mut().offset += 5.0;
        layout(&mut frame, &mut state, &rows);
        for row in &mut rows {
            if row.0 == 32 || row.0 == 30 {
                row.1 *= 2.0;
            }
        }
        layout(&mut frame, &mut state, &rows);
        let anchor = frame.geometry(WidgetId::new(30u32)).unwrap();
        assert_eq!(anchor.y, -5.0);
        layout(&mut frame, &mut state, &rows);
        assert_eq!(frame.geometry(WidgetId::new(30u32)).unwrap(), anchor);

        rows.clear();
        state.mark_dirty();
        layout(&mut frame, &mut state, &rows);
        assert_eq!(state.table.borrow().offset, 0.0);
    }
}
