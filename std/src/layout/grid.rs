use blit::{Axis, Constraints, Layout, LayoutCx, Platform, Point, Sides, Size};

const MAX_SPANNING_COLUMNS: usize = 64;

/// fixed-column row-major grid
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid {
    columns: u16,
    spanning: bool,
    padding: Sides,
    column_gap: f32,
    row_gap: f32,
}

impl Grid {
    pub fn columns(columns: usize) -> Self {
        assert!(columns != 0, "grid must have at least one column");
        Self {
            columns: u16::try_from(columns).expect("too many grid columns"),
            spanning: false,
            padding: Sides::all(0.0),
            column_gap: 0.0,
            row_gap: 0.0,
        }
    }

    pub const fn spanning(mut self) -> Self {
        assert!(
            self.columns as usize <= MAX_SPANNING_COLUMNS,
            "spanning grid supports at most 64 columns"
        );
        self.spanning = true;
        self
    }

    pub const fn padding(mut self, padding: Sides) -> Self {
        self.padding = padding;
        self
    }

    pub const fn gap(mut self, gap: f32) -> Self {
        self.column_gap = gap;
        self.row_gap = gap;
        self
    }

    pub const fn column_gap(mut self, gap: f32) -> Self {
        self.column_gap = gap;
        self
    }

    pub const fn row_gap(mut self, gap: f32) -> Self {
        self.row_gap = gap;
        self
    }

    pub fn placer(self) -> GridPlacer {
        assert!(
            self.spanning,
            "grid spans must be enabled with Grid::spanning"
        );
        GridPlacer {
            columns: self.columns,
            next_row: [0; MAX_SPANNING_COLUMNS],
            row: 0,
            column: 0,
        }
    }
}

/// placement and preferred track contribution for a grid child
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridItem {
    row: u16,
    column: u16,
    row_span: u16,
    column_span: u16,
    preferred_width: Option<f32>,
    preferred_height: Option<f32>,
}

impl Default for GridItem {
    fn default() -> Self {
        Self {
            row: 0,
            column: 0,
            row_span: 1,
            column_span: 1,
            preferred_width: None,
            preferred_height: None,
        }
    }
}

impl GridItem {
    pub const fn preferred_width(mut self, width: f32) -> Self {
        self.preferred_width = Some(width);
        self
    }

    pub const fn preferred_height(mut self, height: f32) -> Self {
        self.preferred_height = Some(height);
        self
    }
}

pub struct GridPlacer {
    columns: u16,
    next_row: [u16; MAX_SPANNING_COLUMNS],
    row: u16,
    column: u16,
}

impl GridPlacer {
    pub fn place(&mut self, rows: usize, columns: usize) -> GridItem {
        let row_span = u16::try_from(rows).expect("grid row span is too large");
        let column_span = u16::try_from(columns).expect("grid column span is too large");
        assert!(row_span != 0, "grid row span must be nonzero");
        assert!(column_span != 0, "grid column span must be nonzero");
        assert!(
            column_span <= self.columns,
            "grid column span exceeds its column count"
        );

        let grid_columns = self.columns as usize;
        let span = column_span as usize;
        let mut placement = None;
        for column in 0..=grid_columns - span {
            let mut row = if column < self.column as usize {
                self.row.checked_add(1).expect("too many grid rows")
            } else {
                self.row
            };
            for occupied in &self.next_row[column..column + span] {
                row = row.max(*occupied);
            }
            if placement.is_none_or(|best| (row, column) < best) {
                placement = Some((row, column));
            }
        }
        let (row, column) = placement.unwrap();
        let end_row = row.checked_add(row_span).expect("too many grid rows");
        self.next_row[column..column + span].fill(end_row);
        let next_column = column + span;
        if next_column == grid_columns {
            self.row = row.checked_add(1).expect("too many grid rows");
            self.column = 0;
        } else {
            self.row = row;
            self.column = next_column as u16;
        }
        GridItem {
            row,
            column: column as u16,
            row_span,
            column_span,
            preferred_width: None,
            preferred_height: None,
        }
    }
}
impl<P: Platform> Layout<P> for Grid {
    type Item = GridItem;

    fn size_override(&self, item: &mut Self::Item, width: Option<f32>, height: Option<f32>) {
        if let Some(width) = width {
            item.preferred_width = Some(width);
        }
        if let Some(height) = height {
            item.preferred_height = Some(height);
        }
    }

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, constraints: Constraints) -> Size {
        let res = cx.layout_resolution();
        let range = |axis, preferred: Option<f32>, available| {
            if let Some(preferred) = preferred {
                let preferred = res.extent(axis, preferred).max(0.0);
                (preferred, preferred)
            } else {
                (0.0, available)
            }
        };
        let columns = self.columns as usize;
        let padding = res.sides(self.padding);
        let column_gap = res.extent(Axis::Horizontal, self.column_gap).max(0.0);
        let row_gap = res.extent(Axis::Vertical, self.row_gap).max(0.0);
        let horizontal_padding = padding.left + padding.right;
        let vertical_padding = padding.top + padding.bottom;
        let horizontal_gaps = column_gap * columns.saturating_sub(1) as f32;
        let max_height = (constraints.max.height - vertical_padding).max(0.0);
        let mut natural_column_width: f32 = 0.0;

        if self.spanning {
            let mut rows = 0usize;
            for node in cx.children() {
                let item = cx.item(node);
                rows = rows.max(item.row as usize + item.row_span as usize);
                let width = range(Axis::Horizontal, item.preferred_width, f32::INFINITY);
                let height = range(Axis::Vertical, item.preferred_height, max_height);
                let child = cx.layout_child(
                    node,
                    Constraints {
                        min: Size {
                            width: width.0,
                            height: height.0,
                        },
                        max: Size {
                            width: width.1,
                            height: height.1,
                        },
                    },
                );
                let internal_gaps = column_gap * item.column_span.saturating_sub(1) as f32;
                natural_column_width = natural_column_width
                    .max((child.width - internal_gaps).max(0.0) / item.column_span as f32);
            }
            if rows == 0 {
                return constraints.constrain(Size::default());
            }

            let natural_width =
                natural_column_width * columns as f32 + horizontal_gaps + horizontal_padding;
            let width = natural_width.clamp(constraints.min.width, constraints.max.width);
            let cell_width =
                (width - horizontal_padding - horizontal_gaps).max(0.0) / columns as f32;
            let mut row_height: f32 = 0.0;

            for node in cx.children() {
                let item = cx.item(node);
                let child_width = cell_width * item.column_span as f32
                    + column_gap * item.column_span.saturating_sub(1) as f32;
                let height = range(Axis::Vertical, item.preferred_height, max_height);
                let child = cx.constrain_child(
                    node,
                    Constraints {
                        min: Size {
                            width: child_width,
                            height: height.0,
                        },
                        max: Size {
                            width: child_width,
                            height: height.1,
                        },
                    },
                );
                let internal_gaps = row_gap * item.row_span.saturating_sub(1) as f32;
                row_height =
                    row_height.max((child.height - internal_gaps).max(0.0) / item.row_span as f32);
            }

            for node in cx.children() {
                let item = cx.item(node);
                let child_width = cell_width * item.column_span as f32
                    + column_gap * item.column_span.saturating_sub(1) as f32;
                let child_height = row_height * item.row_span as f32
                    + row_gap * item.row_span.saturating_sub(1) as f32;
                cx.constrain_child(
                    node,
                    Constraints::tight(Size {
                        width: child_width,
                        height: child_height,
                    }),
                );
                cx.set_position(
                    node,
                    Point {
                        x: padding.left + item.column as f32 * (cell_width + column_gap),
                        y: padding.top + item.row as f32 * (row_height + row_gap),
                    },
                );
            }

            return constraints.constrain(Size {
                width,
                height: row_height * rows as f32
                    + row_gap * rows.saturating_sub(1) as f32
                    + vertical_padding,
            });
        }

        let count = cx.children().count();
        if count == 0 {
            return constraints.constrain(Size::default());
        }
        let rows = count.div_ceil(columns);

        for node in cx.children() {
            let item = cx.item(node);
            let width = range(Axis::Horizontal, item.preferred_width, f32::INFINITY);
            let height = range(Axis::Vertical, item.preferred_height, max_height);
            let child = cx.layout_child(
                node,
                Constraints {
                    min: Size {
                        width: width.0,
                        height: height.0,
                    },
                    max: Size {
                        width: width.1,
                        height: height.1,
                    },
                },
            );
            natural_column_width = natural_column_width.max(child.width);
        }

        let natural_width =
            natural_column_width * columns as f32 + horizontal_gaps + horizontal_padding;
        let width = natural_width.clamp(constraints.min.width, constraints.max.width);
        let cell_width = (width - horizontal_padding - horizontal_gaps).max(0.0) / columns as f32;

        for node in cx.children() {
            let item = cx.item(node);
            let height = range(Axis::Vertical, item.preferred_height, max_height);
            cx.constrain_child(
                node,
                Constraints {
                    min: Size {
                        width: cell_width,
                        height: height.0,
                    },
                    max: Size {
                        width: cell_width,
                        height: height.1,
                    },
                },
            );
        }

        let mut natural_height = vertical_padding + row_gap * rows.saturating_sub(1) as f32;
        let mut children = cx.children().peekable();
        let mut y = padding.top;
        while children.peek().is_some() {
            let row = children.clone();
            let mut row_count = 0usize;
            let mut row_height: f32 = 0.0;
            while row_count < columns
                && let Some(node) = children.next()
            {
                row_height = row_height.max(cx.size(node).height);
                row_count += 1;
            }
            natural_height += row_height;

            for (column, node) in row.take(row_count).enumerate() {
                cx.constrain_child(
                    node,
                    Constraints::tight(Size {
                        width: cell_width,
                        height: row_height,
                    }),
                );
                cx.set_position(
                    node,
                    Point {
                        x: padding.left + column as f32 * (cell_width + column_gap),
                        y,
                    },
                );
            }
            y += row_height + row_gap;
        }

        constraints.constrain(Size {
            width,
            height: natural_height,
        })
    }
}
