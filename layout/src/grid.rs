use blit::{Axis, Constraints, LayoutCx, LogicalPoint, LogicalSize};

use crate::{Padding, Unit, flow_constraints};

blit::builder! {
    #[const]
    /// fixed-column row-major grid
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout<U: Unit, const N: usize = 64> {
        new(columns: u16),
        spanning: bool = false,
        padding: Padding<U> = Padding::all(U::ZERO),
        column_gap: U = U::ZERO,
        row_gap: U = U::ZERO,
    }
}

impl<U: Unit, const N: usize> Layout<U, N> {
    pub const fn gap(self, gap: U) -> Self {
        self.column_gap(gap).row_gap(gap)
    }
}

blit::builder! {
    #[const]
    /// placement and preferred track contribution for a grid child
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Item<U: Unit> {
        new(),
        row_span: u16 = 1,
        column_span: u16 = 1,
        #[option]
        preferred_width: U,
        #[option]
        preferred_height: U,
    }
}

impl<C, U: Unit, const N: usize> blit::Layout<C> for Layout<U, N> {
    type Item = Item<U>;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints) -> LogicalSize {
        assert!(
            !self.spanning || self.columns as usize <= N,
            "grid column count exceeds spanning capacity"
        );
        let range = |preferred: Option<U>, available| {
            if let Some(preferred) = preferred {
                let preferred = preferred.into_float().max(0.0);
                (preferred, preferred)
            } else {
                (0.0, available)
            }
        };
        let columns = self.columns as usize;
        let padding: blit::Sides = self.padding.into();
        let column_gap = self.column_gap.into_float().max(0.0);
        let row_gap = self.row_gap.into_float().max(0.0);
        let horizontal_padding = padding.left + padding.right;
        let vertical_padding = padding.top + padding.bottom;
        let horizontal_gaps = column_gap * columns.saturating_sub(1) as f32;
        let max_height = (U::round(constraints.max.height) - vertical_padding).max(0.0);
        if columns == 0 || cx.children().next().is_none() {
            return constraints.constrain(padding.size());
        }

        let width = if constraints.min.width == constraints.max.width {
            constraints.min.width
        } else {
            let mut natural_column_width: f32 = 0.0;
            for child in cx.children() {
                let item = cx.item(child);
                let width = range(item.preferred_width, f32::INFINITY);
                let height = range(item.preferred_height, max_height);
                let child_size = cx.layout_child(child, flow_constraints::<U>(Axis::Horizontal, width, height));
                let span = if self.spanning {
                    item.column_span.clamp(1, self.columns)
                } else {
                    1
                };
                let internal_gaps = column_gap * span.saturating_sub(1) as f32;
                natural_column_width =
                    natural_column_width.max((U::round(child_size.width) - internal_gaps).max(0.0) / span as f32);
            }

            let natural_width = natural_column_width * columns as f32 + horizontal_gaps + horizontal_padding;
            natural_width.clamp(constraints.min.width, constraints.max.width)
        };
        let cell_width = (U::round(width) - horizontal_padding - horizontal_gaps).max(0.0) / columns as f32;
        let column_start = |column: usize| U::round(padding.left + column as f32 * (cell_width + column_gap));
        let column_width = |column: usize, span: usize| {
            (U::round(padding.left + (column + span) as f32 * (cell_width + column_gap) - column_gap)
                - column_start(column))
            .max(0.0)
        };

        if self.spanning {
            let mut row_height: f32 = 0.0;

            for child in cx.children() {
                let item = cx.item(child);
                let row_span = item.row_span.max(1);
                let column_span = item.column_span.clamp(1, self.columns);
                let assigned_width = column_width(0, column_span as usize);
                let height = range(item.preferred_height, max_height);
                let child_size = cx.layout_child(
                    child,
                    flow_constraints::<U>(Axis::Horizontal, (assigned_width, assigned_width), height),
                );
                let internal_gaps = row_gap * (row_span - 1) as f32;
                row_height = row_height.max((U::round(child_size.height) - internal_gaps).max(0.0) / row_span as f32);
            }

            let mut column_rows = [0usize; N];
            let mut cursor_row = 0usize;
            let mut cursor_column = 0usize;
            let mut rows = 0usize;
            for child in cx.children() {
                let item = cx.item(child);
                let row_span = item.row_span.max(1) as usize;
                let span = item.column_span.clamp(1, self.columns) as usize;
                let column = cursor_column;
                let (row, column) = if column + span <= columns
                    && column_rows[column..column + span].iter().all(|row| *row <= cursor_row)
                {
                    (cursor_row, column)
                } else {
                    let mut placement = None;
                    for column in 0..=columns - span {
                        let mut row = if column < cursor_column {
                            cursor_row + 1
                        } else {
                            cursor_row
                        };
                        for occupied in &column_rows[column..column + span] {
                            row = row.max(*occupied);
                        }
                        if placement.is_none_or(|best| (row, column) < best) {
                            placement = Some((row, column));
                        }
                    }
                    placement.unwrap()
                };

                let end_row = row + row_span;
                column_rows[column..column + span].fill(end_row);
                let next_column = column + span;
                if next_column == columns {
                    cursor_row = row + 1;
                    cursor_column = 0;
                } else {
                    cursor_row = row;
                    cursor_column = next_column;
                }
                rows = rows.max(end_row);
                let assigned_width = column_width(column, span);
                let assigned_height = (U::round(padding.top + end_row as f32 * (row_height + row_gap) - row_gap)
                    - U::round(padding.top + row as f32 * (row_height + row_gap)))
                .max(0.0);
                let child_size = LogicalSize {
                    width: assigned_width,
                    height: assigned_height,
                };
                if cx.size(child) != child_size {
                    cx.layout_child(child, Constraints::tight(child_size));
                }
                cx.set_position(
                    child,
                    LogicalPoint::new(
                        column_start(column),
                        U::round(padding.top + row as f32 * (row_height + row_gap)),
                    ),
                );
            }

            return constraints.constrain(LogicalSize {
                width,
                height: row_height * rows as f32 + row_gap * rows.saturating_sub(1) as f32 + vertical_padding,
            });
        }

        let mut count = 0usize;
        for child in cx.children() {
            count += 1;
            let item = cx.item(child);
            let height = range(item.preferred_height, max_height);
            cx.layout_child(
                child,
                flow_constraints::<U>(Axis::Horizontal, (column_width(0, 1), column_width(0, 1)), height),
            );
        }
        let rows = count.div_ceil(columns);

        let mut natural_height = vertical_padding + row_gap * rows.saturating_sub(1) as f32;
        let mut children = cx.children().peekable();
        let mut y = padding.top;
        while children.peek().is_some() {
            let row = children.clone();
            let mut row_count = 0usize;
            let mut row_height: f32 = 0.0;
            while row_count < columns
                && let Some(child) = children.next()
            {
                row_height = row_height.max(U::round(cx.size(child).height));
                row_count += 1;
            }
            natural_height += row_height;

            for (column, child) in row.take(row_count).enumerate() {
                let child_size = LogicalSize::new(column_width(column, 1), row_height);
                if cx.size(child) != child_size {
                    cx.layout_child(child, Constraints::tight(child_size));
                }
                cx.set_position(child, LogicalPoint::new(column_start(column), y));
            }
            y += row_height + row_gap;
        }

        constraints.constrain(LogicalSize {
            width,
            height: natural_height,
        })
    }
}
