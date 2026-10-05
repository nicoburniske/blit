use blit::{Axis, Constraints, Context, LayoutCx, Point, Scalar, Sides, Size};

use super::flow_constraints;

blit::builder! {
    /// fixed column row major grid
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout<T: Scalar> {
        new(columns: u32),
        padding: Sides<T> = Sides::all(T::ZERO),
        column_gap: T = T::ZERO,
        row_gap: T = T::ZERO,
    }
}

impl<T: Scalar> Layout<T> {
    pub const fn gap(mut self, gap: T) -> Self {
        self.column_gap = gap;
        self.row_gap = gap;
        self
    }
}

blit::builder! {
    /// spans and contributions to track sizing for a grid child
    /// final cells can be larger than the supplied width and height
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Item<T: Scalar> {
        new(),
        @optional {
            width: T,
            height: T,
        },
        row_span: u32 = 1,
        column_span: u32 = 1,
    }
}

pub fn new<T: Scalar>(columns: u32) -> Layout<T> {
    Layout::new(columns)
}

pub fn item<T: Scalar>() -> Item<T> {
    Item::new()
}

impl<C: Context<Scalar = T>, T: Scalar> blit::Layout<C> for Layout<T> {
    type Item = Item<T>;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints<T>) -> Size<T> {
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
            let mut rows = 0usize;
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
                    (0..=columns - span)
                        .map(|column| {
                            let row = column_rows[column..column + span]
                                .iter()
                                .copied()
                                .fold(cursor_row + usize::from(column < cursor_column), usize::max);
                            (row, column)
                        })
                        .min()
                        .unwrap()
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
        let padding = self.padding;
        let column_gap = self.column_gap.max(T::ZERO);
        let row_gap = self.row_gap.max(T::ZERO);
        if row_count == 0 {
            return bounds.constrain(padding.size());
        }
        let horizontal_gaps = column_gap.repeat(columns.saturating_sub(1));
        let maximum = bounds.shrink(padding.size()).max;
        let range = |preferred: Option<T>, available| {
            if let Some(preferred) = preferred {
                let preferred = preferred.max(T::ZERO);
                (preferred, preferred)
            } else {
                (T::ZERO, available)
            }
        };
        let width = if bounds.min.width == bounds.max.width {
            bounds.min.width
        } else {
            let mut column_width: f32 = 0.0;
            for child in cx.children() {
                let item = cx.item(child);
                let size = cx.layout_child(
                    child,
                    flow_constraints(
                        Axis::Horizontal,
                        range(item.width, maximum.width),
                        range(item.height, maximum.height),
                    ),
                );
                let span = item.column_span as usize;
                column_width = column_width.max(
                    (size.width - column_gap.repeat(span.saturating_sub(1)))
                        .max(T::ZERO)
                        .to_f32()
                        / span as f32,
                );
            }
            (T::from_f32(column_width * columns as f32) + horizontal_gaps + padding.size().width)
                .clamp(bounds.min.width, bounds.max.width)
        };
        let cell = (width - padding.size().width - horizontal_gaps).max(T::ZERO).to_f32() / columns as f32;
        let mut height = T::ZERO;
        let mut children = cx.children();
        let mut index = 0;
        let mut y = padding.top;
        while index < count {
            let group = if spanning { count } else { columns.min(count - index) };
            let mut row_height = T::ZERO;
            let mut row_track: f32 = 0.0;
            for (offset, child) in children.take(group).enumerate() {
                let column = if spanning { positions[index + offset].1 } else { offset };
                let item = cx.item(child);
                let span = item.column_span as usize;
                let assigned = T::from_f32((column + span) as f32 * cell) - T::from_f32(column as f32 * cell)
                    + column_gap.repeat(span.saturating_sub(1));
                let size = cx.layout_child(
                    child,
                    flow_constraints(
                        Axis::Horizontal,
                        (assigned, assigned),
                        range(item.height, maximum.height),
                    ),
                );
                if spanning {
                    let span = item.row_span as usize;
                    row_track = row_track.max(
                        (size.height - row_gap.repeat(span.saturating_sub(1)))
                            .max(T::ZERO)
                            .to_f32()
                            / span as f32,
                    );
                } else {
                    row_height = row_height.max(size.height);
                }
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
                        T::from_f32((row + row_span) as f32 * row_track) - T::from_f32(row as f32 * row_track)
                            + row_gap.repeat(row_span.saturating_sub(1))
                    } else {
                        row_height
                    },
                );
                if current != child_size {
                    cx.layout_child(child, Constraints::tight(child_size));
                }
                cx.set_child_position(
                    child,
                    Point::new(
                        padding.left + T::from_f32(column as f32 * cell) + column_gap.repeat(column),
                        if spanning {
                            padding.top + T::from_f32(row as f32 * row_track) + row_gap.repeat(row)
                        } else {
                            y
                        },
                    ),
                );
            }
            height += if spanning {
                T::from_f32(row_track * row_count as f32)
            } else {
                row_height
            };
            y += row_height + row_gap;
            index += group;
        }
        bounds.constrain(Size::new(
            width,
            height + row_gap.repeat(row_count.saturating_sub(1)) + padding.size().height,
        ))
    }
}
