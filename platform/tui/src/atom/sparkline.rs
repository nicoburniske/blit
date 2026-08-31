use std::{cell::RefCell, rc::Rc};

use blit::{Atom, Constraints, LogicalRect, Size};
use blit_tui_render::{
    color::Color,
    surface::{Cell, CellStyle},
};

use crate::TuiPlatform;

pub struct Sparkline {
    pub data: Rc<RefCell<Vec<u64>>>,
    pub maximum: Option<u64>,
    pub color: Color,
    pub background: Option<Color>,
}

impl Sparkline {
    pub fn new(data: Rc<RefCell<Vec<u64>>>) -> Self {
        Self {
            data,
            maximum: None,
            color: Color::Reset,
            background: None,
        }
    }

    pub const fn maximum(mut self, maximum: u64) -> Self {
        self.maximum = Some(maximum);
        self
    }

    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub const fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
}

impl Atom<TuiPlatform> for Sparkline {
    fn measure(&self, _: &mut TuiPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::new(self.data.borrow().len() as f32, 1.0))
    }

    fn paint(&self, platform: &mut TuiPlatform, area: LogicalRect) {
        const LEVELS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let data = self.data.borrow();
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

    fn measure_depends_on_constraints(&self) -> bool {
        false
    }
}

blit::impl_atom_widgets!(TuiPlatform => Sparkline);
