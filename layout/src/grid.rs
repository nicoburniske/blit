use blit::{Axis, Constraints, LayoutCx, Point, Sides, Size};

use super::flow_constraints;
use crate::{Context, layout_child};

blit::builder! {
    /// fixed column row major grid
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout {
        new(columns: u32),
        padding: Sides = Sides::all(0.0),
        column_gap: f32 = 0.0,
        row_gap: f32 = 0.0,
    }
}

impl Layout {
    pub const fn gap(mut self, gap: f32) -> Self {
        self.column_gap = gap;
        self.row_gap = gap;
        self
    }
}

blit::builder! {
    /// spans and contributions to track sizing for a grid child
    /// final cells can be larger than the supplied width and height
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Item {
        new(),
        @optional {
            width: f32,
            height: f32,
        },
        row_span: u32 = 1,
        column_span: u32 = 1,
    }
}

pub fn new(columns: u32) -> Layout {
    Layout::new(columns)
}

pub fn item() -> Item {
    Item::new()
}

impl<C: Context> blit::Layout<C> for Layout {
    type Item = Item;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> Size {
        assert!(self.columns != 0, "grid must have at least one column");
        let columns = self.columns as usize;
        let mut count = 0usize;
        let mut spanning = false;
        for child in cx.children() {
            let item = cx.item(child);
            assert!(
                item.row_span != 0 && item.column_span != 0,
                "grid spans must be nonzero"
            );
            spanning |= item.row_span != 1 || item.column_span != 1;
            count += 1;
        }
        let mut positions = cx.scratch(if spanning { count } else { 0 }, (0usize, 0usize));
        let row_count = if spanning {
            let mut column_rows = cx.scratch(columns, 0usize);
            let mut cursor = (0usize, 0usize);
            let mut rows = 0;
            for (child, position) in cx.children().zip(positions.iter_mut()) {
                let item = cx.item(child);
                assert!(
                    item.column_span <= self.columns,
                    "grid column span exceeds its column count"
                );
                let span = item.column_span as usize;
                let (cursor_row, cursor_column) = cursor;
                let (row, column) = if span <= columns - cursor_column
                    && column_rows[cursor_column..cursor_column + span]
                        .iter()
                        .all(|row| *row <= cursor_row)
                {
                    cursor
                } else {
                    let mut placement = None;
                    for column in 0..=columns - span {
                        let row = column_rows[column..column + span]
                            .iter()
                            .copied()
                            .fold(cursor_row + usize::from(column < cursor_column), usize::max);
                        if placement.is_none_or(|best| (row, column) < best) {
                            placement = Some((row, column));
                        }
                    }
                    placement.unwrap()
                };
                column_rows[column..column + span]
                    .fill(row.checked_add(item.row_span as usize).expect("too many grid rows"));
                cursor = if column + span == columns {
                    (row + 1, 0)
                } else {
                    (row, column + span)
                };
                *position = (row, column);
                rows = rows.max(row + item.row_span as usize);
            }
            rows
        } else {
            count.div_ceil(columns)
        };
        let padding = crate::round_padding::<C>(self.padding);
        let column_gap = C::round(self.column_gap).max(0.0);
        let row_gap = C::round(self.row_gap).max(0.0);
        if row_count == 0 {
            return bounds.constrain(padding.size());
        }
        let horizontal_gaps = column_gap * columns.saturating_sub(1) as f32;
        let maximum = (bounds.max - padding.size()).max(Size::ZERO);
        let range = |preferred: Option<f32>, available| {
            if let Some(preferred) = preferred {
                let preferred = C::round(preferred).max(0.0);
                (preferred, preferred)
            } else {
                (0.0, available)
            }
        };
        let width = if bounds.min.width == bounds.max.width {
            bounds.min.width
        } else {
            let mut column_width: f32 = 0.0;
            for child in cx.children() {
                let item = cx.item(child);
                let size = layout_child(
                    cx,
                    child,
                    flow_constraints(
                        Axis::Horizontal,
                        range(item.width, maximum.width),
                        range(item.height, maximum.height),
                    ),
                );
                let span = item.column_span as usize;
                column_width =
                    column_width.max((size.width - column_gap * span.saturating_sub(1) as f32).max(0.0) / span as f32);
            }
            (column_width * columns as f32 + horizontal_gaps + padding.size().width)
                .clamp(bounds.min.width, bounds.max.width)
        };
        let cell = (width - padding.size().width - horizontal_gaps).max(0.0) / columns as f32;
        let mut height = 0.0;
        let mut children = cx.children();
        let mut index = 0;
        let mut y = padding.top;
        while index < count {
            let group = if spanning { count } else { columns.min(count - index) };
            let start = children;
            let mut row_height: f32 = 0.0;
            for (offset, child) in start.take(group).enumerate() {
                let column = if spanning { positions[index + offset].1 } else { offset };
                let item = cx.item(child);
                let span = item.column_span as usize;
                let assigned = C::round((column + span) as f32 * cell) - C::round(column as f32 * cell)
                    + column_gap * span.saturating_sub(1) as f32;
                let size = layout_child(
                    cx,
                    child,
                    flow_constraints(
                        Axis::Horizontal,
                        (assigned, assigned),
                        range(item.height, maximum.height),
                    ),
                );
                row_height = row_height.max(if spanning {
                    let span = item.row_span as usize;
                    (size.height - row_gap * span.saturating_sub(1) as f32).max(0.0) / span as f32
                } else {
                    size.height
                });
            }
            for (offset, child) in children.by_ref().take(group).enumerate() {
                let (row, column) = if spanning {
                    positions[index + offset]
                } else {
                    (0, offset)
                };
                let current = cx.size(child);
                let child_size = Size::new(
                    current.width,
                    if spanning {
                        let row_span = cx.item(child).row_span as usize;
                        C::round((row + row_span) as f32 * row_height) - C::round(row as f32 * row_height)
                            + row_gap * row_span.saturating_sub(1) as f32
                    } else {
                        row_height
                    },
                );
                if current != child_size {
                    layout_child(cx, child, Constraints::tight(child_size));
                }
                cx.set_child_position(
                    child,
                    Point::new(
                        padding.left + C::round(column as f32 * cell) + column as f32 * column_gap,
                        if spanning {
                            padding.top + C::round(row as f32 * row_height) + row as f32 * row_gap
                        } else {
                            y
                        },
                    ),
                );
            }
            height += if spanning {
                C::round(row_height * row_count as f32)
            } else {
                row_height
            };
            y += row_height + row_gap;
            index += group;
        }
        bounds.constrain(Size::new(
            width,
            height + row_gap * row_count.saturating_sub(1) as f32 + padding.size().height,
        ))
    }
}
