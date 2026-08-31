use blit::{Atom, Constraints, LogicalRect, Size};
use blit_tui_render::{
    color::Color,
    surface::{Cell, CellStyle},
};

use crate::TuiPlatform;

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

impl Atom<TuiPlatform> for Shadow {
    fn measure(&self, _: &mut TuiPlatform, _: Constraints) -> Size {
        Size::ZERO
    }

    fn paint(&self, platform: &mut TuiPlatform, area: LogicalRect) {
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
        let mut cells = platform.cells(shifted);
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

    fn measure_depends_on_constraints(&self) -> bool {
        false
    }
}

blit::impl_atom_widgets!(TuiPlatform => Shadow);
