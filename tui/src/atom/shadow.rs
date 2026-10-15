use blit::{Atom, AtomCx, Constraints, PhysicalRect, Size};

use crate::{
    Context,
    cell::{Cell, CellStyle},
    color::Color,
};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Shadow {
        new(color: Color),
        offset_x: i32 = 1,
        offset_y: i32 = 1,
    }
}

impl Shadow {
    pub fn offset(mut self, x: i32, y: i32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }
}

blit::atom_widget!(Shadow);

impl<C: Context> Atom<C> for Shadow {
    fn measure(&self, _: &mut AtomCx<'_, C>, _: Constraints<i32>) -> Size<i32> {
        Size::ZERO
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: PhysicalRect) {
        let shifted = PhysicalRect {
            x: area.x + self.offset_x,
            y: area.y + self.offset_y,
            ..area
        };
        let left = area.x as isize;
        let top = area.y as isize;
        let right = (area.x + area.width) as isize;
        let bottom = (area.y + area.height) as isize;
        let origin_x = shifted.x as isize;
        let origin_y = shifted.y as isize;
        let mut cells = cx.context_mut().as_mut().cells(shifted);
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

    fn paint_bounds(&self, area: PhysicalRect) -> PhysicalRect {
        PhysicalRect {
            x: area.x + self.offset_x,
            y: area.y + self.offset_y,
            ..area
        }
    }
}
