use blit::{Atom, Constraints, LogicalRect, Size};
use blit_std::ReadSlice;
use blit_tui_render::{
    cell::{Cell, CellStyle},
    color::Color,
};

use crate::TuiPlatform;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bar {
    pub value: u64,
    pub label: String,
}

impl Bar {
    pub fn new(value: u64, label: String) -> Self {
        Self { value, label }
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct BarChart<D> {
        new(bars: D),
        @optional {
            maximum: u64,
            background: Color,
        },
        bar_width: usize = 3,
        gap: usize = 1,
        color: Color = Color::Reset,
        label_color: Color = Color::Reset,
    }
}

impl<D> Atom<TuiPlatform> for BarChart<D>
where
    D: ReadSlice<Item = Bar> + 'static,
{
    fn measure(&self, _: &mut TuiPlatform, constraints: Constraints) -> Size {
        let width = self
            .bars
            .read()
            .len()
            .saturating_mul(self.bar_width + self.gap)
            .saturating_sub(self.gap);
        constraints.constrain(Size::new(width as f32, 5.0))
    }

    fn paint(&self, platform: &mut TuiPlatform, area: LogicalRect) {
        const LEVELS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let bars = self.bars.read();
        let mut cells = platform.cells(area);
        let width = cells.columns();
        let rows = cells.rows();
        if let Some(background) = self.background {
            cells.clear(Cell::new(' ').style(CellStyle::new().background(background)));
        }
        if rows == 0 || self.bar_width == 0 {
            return;
        }
        let chart_rows = rows.saturating_sub(1);
        let maximum = self
            .maximum
            .unwrap_or_else(|| bars.iter().map(|bar| bar.value).max().unwrap_or(0))
            .max(1);
        let bar_style = if let Some(background) = self.background {
            CellStyle::new()
                .foreground(self.color)
                .background(background)
        } else {
            CellStyle::new().foreground(self.color)
        };
        let label_style = if let Some(background) = self.background {
            CellStyle::new()
                .foreground(self.label_color)
                .background(background)
        } else {
            CellStyle::new().foreground(self.label_color)
        };
        for (index, bar) in bars.iter().enumerate() {
            let start = index * (self.bar_width + self.gap);
            if start >= width {
                break;
            }
            let mut eighths =
                ((bar.value as u128 * chart_rows as u128 * 8) / maximum as u128) as usize;
            for y in (0..chart_rows).rev() {
                if eighths == 0 {
                    break;
                }
                let level = eighths.min(8);
                for x in start..(start + self.bar_width).min(width) {
                    cells.set_cell(x, y, Cell::new(LEVELS[level]).style(bar_style));
                }
                eighths = eighths.saturating_sub(8);
            }
            let label_width = bar.label.chars().count().min(self.bar_width);
            let label_start = start + self.bar_width.saturating_sub(label_width) / 2;
            for (offset, character) in bar.label.chars().take(label_width).enumerate() {
                cells.set_cell(
                    label_start + offset,
                    rows - 1,
                    Cell::new(character).style(label_style),
                );
            }
        }
    }

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        area
    }
}
