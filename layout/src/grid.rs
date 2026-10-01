use blit::{
    Axis, Child, Constraints, IntrinsicQuery, IntrinsicSize, LayoutCx, LogicalPoint, LogicalSize, MeasureCx,
    layout::Children,
};

use crate::{Padding, Unit};

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

impl<U: Unit, const N: usize> blit::Layout for Layout<U, N> {
    type Item = Item<U>;

    fn intrinsic(&self, cx: &mut MeasureCx<'_, Self::Item>, query: IntrinsicQuery) -> IntrinsicSize {
        let columns = self.columns.max(1) as usize;
        assert!(
            !self.spanning || columns <= N,
            "grid column count exceeds spanning capacity"
        );
        let padding: blit::Sides = self.padding.into();
        let column_gap = self.column_gap.into_float().max(0.0);
        let row_gap = self.row_gap.into_float().max(0.0);
        let horizontal_gaps = column_gap * columns.saturating_sub(1) as f32;
        if cx.children().next().is_none() {
            let extent = query.axis.extent(padding.size());
            return IntrinsicSize::uniform(extent);
        }
        if query.axis == Axis::Horizontal {
            let width = self.natural_width(cx);
            return IntrinsicSize::new(U::round(width.min), U::round(width.preferred));
        }
        let width = query
            .cross
            .unwrap_or_else(|| U::round(self.natural_width(cx).preferred));
        let cell_width = (U::round(width) - padding.size().width - horizontal_gaps).max(0.0) / columns as f32;
        let assigned_width = |column, span| track_extent::<U>(padding.left, cell_width, column_gap, column, span);
        let mut result = IntrinsicSize::uniform(padding.size().height);
        let mut row_height = IntrinsicSize::default();
        let mut rows = 0;
        for (child, (row, column, row_span, span)) in placements::<U, N>(cx.children(), columns, self.spanning) {
            if !self.spanning && row >= rows && rows != 0 {
                result.min += row_height.min + row_gap;
                result.preferred += row_height.preferred + row_gap;
                row_height = IntrinsicSize::default();
            }
            rows = rows.max(row + row_span);
            let size = contribution(cx, child, Axis::Vertical, Some(assigned_width(column, span)));
            row_height.min = row_height.min.max(track_size::<U>(size.min, row_span, row_gap));
            row_height.preferred = row_height
                .preferred
                .max(track_size::<U>(size.preferred, row_span, row_gap));
        }
        if self.spanning {
            let gaps = row_gap * rows.saturating_sub(1) as f32;
            result.min += U::round(row_height.min * rows as f32 + gaps);
            result.preferred += U::round(row_height.preferred * rows as f32 + gaps);
        } else {
            result.min += row_height.min;
            result.preferred += row_height.preferred;
        }
        result
    }

    fn layout(&self, cx: &mut LayoutCx<'_, Self::Item>, constraints: Constraints) -> LogicalSize {
        let columns = self.columns.max(1) as usize;
        assert!(
            !self.spanning || columns <= N,
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
        let padding: blit::Sides = self.padding.into();
        let column_gap = self.column_gap.into_float().max(0.0);
        let row_gap = self.row_gap.into_float().max(0.0);
        let horizontal_padding = padding.left + padding.right;
        let vertical_padding = padding.top + padding.bottom;
        let horizontal_gaps = column_gap * columns.saturating_sub(1) as f32;
        let max_height = (U::round(constraints.max.height) - vertical_padding).max(0.0);
        if cx.children().next().is_none() {
            return constraints.constrain(padding.size());
        }

        let width = if constraints.min.width == constraints.max.width {
            constraints.min.width
        } else {
            self.natural_width(cx)
                .preferred
                .clamp(constraints.min.width, constraints.max.width)
        };
        let cell_width = (U::round(width) - horizontal_padding - horizontal_gaps).max(0.0) / columns as f32;
        let column_start = |column: usize| U::round(padding.left + column as f32 * (cell_width + column_gap));
        let column_width = |column, span| track_extent::<U>(padding.left, cell_width, column_gap, column, span);

        let mut row_height: f32 = 0.0;
        let mut rows = 0usize;
        for (child, (row, column, row_span, span)) in placements::<U, N>(cx.children(), columns, self.spanning) {
            rows = rows.max(row + row_span);
            let assigned_width = column_width(column, span);
            let height = range(child.item.preferred_height, max_height);
            let child_size = cx.layout_child(
                child.id,
                Constraints {
                    min: LogicalSize::new(assigned_width, height.0),
                    max: LogicalSize::new(assigned_width, height.1),
                },
            );
            if self.spanning {
                row_height = row_height.max(track_size::<U>(child_size.height, row_span, row_gap));
            }
        }
        if self.spanning {
            for (child, (row, column, row_span, span)) in placements::<U, N>(cx.children(), columns, true) {
                let child_size = LogicalSize::new(
                    column_width(column, span),
                    track_extent::<U>(padding.top, row_height, row_gap, row, row_span),
                );
                if cx.size(child.id) != child_size {
                    cx.layout_child(child.id, Constraints::tight(child_size));
                }
                cx.set_position(
                    child.id,
                    LogicalPoint::new(
                        column_start(column),
                        U::round(padding.top + row as f32 * (row_height + row_gap)),
                    ),
                );
            }

            return constraints.constrain(LogicalSize {
                width,
                height: U::round(row_height * rows as f32 + row_gap * rows.saturating_sub(1) as f32 + vertical_padding),
            });
        }

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
                row_height = row_height.max(U::round(cx.size(child.id).height));
                row_count += 1;
            }
            natural_height += row_height;

            for (column, child) in row.take(row_count).enumerate() {
                let child_size = LogicalSize::new(column_width(column, 1), row_height);
                if cx.size(child.id) != child_size {
                    cx.layout_child(child.id, Constraints::tight(child_size));
                }
                cx.set_position(child.id, LogicalPoint::new(column_start(column), y));
            }
            y += row_height + row_gap;
        }

        constraints.constrain(LogicalSize {
            width,
            height: natural_height,
        })
    }
}

impl<U: Unit, const N: usize> Layout<U, N> {
    fn natural_width(&self, cx: &mut MeasureCx<'_, Item<U>>) -> IntrinsicSize {
        let columns = self.columns.max(1) as usize;
        let gap = self.column_gap.into_float().max(0.0);
        let padding: blit::Sides = self.padding.into();
        let mut track = IntrinsicSize::default();
        for child in cx.children() {
            let span = if self.spanning {
                (child.item.column_span as usize).clamp(1, columns)
            } else {
                1
            };
            let size = contribution(
                cx,
                child,
                Axis::Horizontal,
                child.item.preferred_height.map(|value| value.into_float().max(0.0)),
            );
            track.min = track.min.max(track_size::<U>(size.min, span, gap));
            track.preferred = track.preferred.max(track_size::<U>(size.preferred, span, gap));
        }
        let extra = gap * columns.saturating_sub(1) as f32 + padding.size().width;
        IntrinsicSize::new(
            track.min * columns as f32 + extra,
            track.preferred * columns as f32 + extra,
        )
    }
}

fn placements<U: Unit, const N: usize>(
    children: Children<'_, Item<U>>,
    columns: usize,
    spanning: bool,
) -> impl Iterator<Item = (Child<'_, Item<U>>, (usize, usize, usize, usize))> {
    let mut column_rows = [0usize; N];
    let mut cursor = (0, 0);
    children.enumerate().map(move |(index, child)| {
        if !spanning {
            return (child, (index / columns, index % columns, 1, 1));
        }
        let (cursor_row, cursor_column) = cursor;
        let row_span = child.item.row_span.max(1) as usize;
        let span = (child.item.column_span as usize).clamp(1, columns);
        let (row, column) = if cursor_column + span <= columns
            && column_rows[cursor_column..cursor_column + span]
                .iter()
                .all(|row| *row <= cursor_row)
        {
            (cursor_row, cursor_column)
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
        column_rows[column..column + span].fill(row + row_span);
        cursor = if column + span == columns {
            (row + 1, 0)
        } else {
            (row, column + span)
        };
        (child, (row, column, row_span, span))
    })
}

fn contribution<U: Unit>(
    cx: &mut MeasureCx<'_, Item<U>>,
    child: Child<'_, Item<U>>,
    axis: Axis,
    cross: Option<f32>,
) -> IntrinsicSize {
    let preferred = match axis {
        Axis::Horizontal => child.item.preferred_width,
        Axis::Vertical => child.item.preferred_height,
    };
    if let Some(preferred) = preferred {
        let preferred = preferred.into_float().max(0.0);
        IntrinsicSize::uniform(preferred)
    } else {
        cx.intrinsic(child.id, IntrinsicQuery { axis, cross })
    }
}

fn track_extent<U: Unit>(left: f32, cell: f32, gap: f32, column: usize, span: usize) -> f32 {
    (U::round(left + (column + span) as f32 * (cell + gap) - gap) - U::round(left + column as f32 * (cell + gap)))
        .max(0.0)
}

#[inline]
fn track_size<U: Unit>(extent: f32, span: usize, gap: f32) -> f32 {
    (U::round(extent) - gap * span.saturating_sub(1) as f32).max(0.0) / span as f32
}
