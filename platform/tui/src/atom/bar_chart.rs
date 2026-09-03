use std::{cell::RefCell, rc::Rc};

use blit::{Atom, Constraints, LogicalRect, Size};
use blit_tui_render::{
    cell::{Cell, CellStyle},
    color::Color,
};

use crate::TuiPlatform;

pub struct Bar {
    pub value: u64,
    pub label: String,
}

impl Bar {
    pub fn new(value: u64, label: String) -> Self {
        Self { value, label }
    }
}

pub struct BarChart {
    pub bars: Rc<RefCell<Vec<Bar>>>,
    pub maximum: Option<u64>,
    pub bar_width: usize,
    pub gap: usize,
    pub color: Color,
    pub label_color: Color,
    pub background: Option<Color>,
}

impl BarChart {
    pub fn new(bars: Rc<RefCell<Vec<Bar>>>) -> Self {
        Self {
            bars,
            maximum: None,
            bar_width: 3,
            gap: 1,
            color: Color::Reset,
            label_color: Color::Reset,
            background: None,
        }
    }

    pub const fn maximum(mut self, maximum: u64) -> Self {
        self.maximum = Some(maximum);
        self
    }

    pub const fn bar_width(mut self, width: usize) -> Self {
        self.bar_width = width;
        self
    }

    pub const fn gap(mut self, gap: usize) -> Self {
        self.gap = gap;
        self
    }

    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub const fn label_color(mut self, color: Color) -> Self {
        self.label_color = color;
        self
    }

    pub const fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
}

impl Atom<TuiPlatform> for BarChart {
    fn measure(&self, _: &mut TuiPlatform, constraints: Constraints) -> Size {
        let width = self
            .bars
            .borrow()
            .len()
            .saturating_mul(self.bar_width + self.gap)
            .saturating_sub(self.gap);
        constraints.constrain(Size::new(width as f32, 5.0))
    }

    fn paint(&self, platform: &mut TuiPlatform, area: LogicalRect) {
        const LEVELS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let bars = self.bars.borrow();
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

    fn measure_depends_on_constraints(&self) -> bool {
        false
    }
}
