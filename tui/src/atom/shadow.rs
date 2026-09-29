use blit::{Atom, Constraints, LogicalRect, Size};

use crate::{
    TuiContext,
    cell::{Cell, CellStyle},
    color::Color,
    renderer::CellRect,
};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Shadow {
        new(color: Color),
        offset_x: f32 = 1.0,
        offset_y: f32 = 1.0,
    }
}

impl Shadow {
    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }
}

impl Atom<TuiContext> for Shadow {
    fn measure(&self, _: &mut TuiContext, _: Constraints) -> Size {
        Size::ZERO
    }

    fn paint(&self, context: &mut TuiContext, area: LogicalRect) {
        let shifted = LogicalRect {
            x: area.x + self.offset_x,
            y: area.y + self.offset_y,
            ..area
        };
        let area = CellRect::from_logical(area);
        let shifted_cells = CellRect::from_logical(shifted);
        let mut cells = context.cells(shifted);
        let style = CellStyle::new().background(self.color);
        for y in 0..cells.rows() {
            for x in 0..cells.columns() {
                let screen_x = shifted_cells.left + x as isize;
                let screen_y = shifted_cells.top + y as isize;
                if (area.left..area.right).contains(&screen_x) && (area.top..area.bottom).contains(&screen_y) {
                    continue;
                }
                cells.set_cell(x, y, Cell::default().style(style));
            }
        }
    }

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        LogicalRect {
            x: area.x + self.offset_x,
            y: area.y + self.offset_y,
            ..area
        }
    }
}
