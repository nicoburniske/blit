use blit::{Atom, Constraints, LogicalRect, Size};
use blit_std::ReadSlice;
use blit_tui_render::{
    cell::{Cell, CellStyle},
    color::Color,
};

use crate::TuiPlatform;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Sparkline<D> {
        new(data: D),
        @optional {
            maximum: u64,
            background: Color,
        },
        color: Color = Color::Reset,
    }
}

impl<D> Atom<TuiPlatform> for Sparkline<D>
where
    D: ReadSlice<Item = u64> + 'static,
{
    fn measure(&self, _: &mut TuiPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::new(self.data.read().len() as f32, 1.0))
    }

    fn paint(&self, platform: &mut TuiPlatform, area: LogicalRect) {
        const LEVELS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let data = self.data.read();
        let mut cells = platform.cells(area);
        let width = cells.columns();
        let rows = cells.rows();
        let style = if let Some(background) = self.background {
            CellStyle::new()
                .foreground(self.color)
                .background(background)
        } else {
            CellStyle::new().foreground(self.color)
        };
        if let Some(background) = self.background {
            cells.clear(Cell::new(' ').style(CellStyle::new().background(background)));
        }
        let maximum = self
            .maximum
            .unwrap_or_else(|| data.iter().copied().max().unwrap_or(0))
            .max(1);
        for (x, value) in data.iter().copied().take(width).enumerate() {
            let mut eighths = ((value as u128 * rows as u128 * 8) / maximum as u128) as usize;
            for y in (0..rows).rev() {
                if eighths == 0 {
                    break;
                }
                let level = eighths.min(8);
                cells.set_cell(x, y, Cell::new(LEVELS[level]).style(style));
                eighths = eighths.saturating_sub(8);
            }
        }
    }

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        area
    }
}
