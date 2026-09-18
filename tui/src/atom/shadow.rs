use crate::{
    cell::{Cell, CellStyle},
    color::Color,
};
use blit::{Atom, Constraints, LogicalRect, Size};

use crate::TuiContext;

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
        let left = area.x.round() as isize;
        let top = area.y.round() as isize;
        let right = (area.x + area.width).round() as isize;
        let bottom = (area.y + area.height).round() as isize;
        let origin_x = shifted.x.round() as isize;
        let origin_y = shifted.y.round() as isize;
        let mut cells = context.cells(shifted);
        let style = CellStyle::new().background(self.color);
        for y in 0..cells.rows() {
            for x in 0..cells.columns() {
                let screen_x = origin_x + x as isize;
                let screen_y = origin_y + y as isize;
                if (left..right).contains(&screen_x) && (top..bottom).contains(&screen_y) {
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
