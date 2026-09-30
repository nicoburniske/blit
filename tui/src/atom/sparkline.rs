use std::{cell::RefCell, rc::Rc};

use blit::{Atom, Axis, Constraints, IntrinsicQuery, IntrinsicSize, LogicalRect, LogicalSize};

use crate::{
    TuiContext,
    cell::{Cell, CellStyle},
    color::Color,
};

blit::builder! {
    pub struct Sparkline {
        new(data: Rc<RefCell<Vec<u64>>>),
        #[option]
        maximum: u64,
        #[option]
        background: Color,
        color: Color = Color::Reset,
    }
}

impl Atom<TuiContext> for Sparkline {
    fn intrinsic(&self, context: &mut TuiContext, query: IntrinsicQuery) -> IntrinsicSize {
        let preferred = query
            .axis
            .extent(self.measure(context, Constraints::loose(LogicalSize::uniform(f32::INFINITY))));
        IntrinsicSize::new(if query.axis == Axis::Vertical { 1.0 } else { 0.0 }, preferred)
    }

    fn measure(&self, _: &mut TuiContext, constraints: Constraints) -> LogicalSize {
        constraints.constrain(LogicalSize::new(self.data.borrow().len() as f32, 1.0))
    }

    fn paint(&self, context: &mut TuiContext, area: LogicalRect) {
        const LEVELS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let data = self.data.borrow();
        let mut cells = context.cells(area);
        let width = cells.columns();
        let rows = cells.rows();
        let style = if let Some(background) = self.background {
            CellStyle::new().foreground(self.color).background(background)
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
}
