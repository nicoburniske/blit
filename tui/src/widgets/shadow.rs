use blit::{PhysicalRect, Ui, Widget, state};

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

impl<C> Widget<C, state::Insert> for Shadow
where
    C: Context,
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, C, state::Insert>) {
        ui.paint(move |cx, area| {
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
            let mut cells = cx.context_mut().as_mut().renderer.cells(shifted);
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
        });
    }
}
