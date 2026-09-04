use std::rc::Rc;

use blit::{Atom, Constraints, LogicalRect, Size};
use blit_tui_render::{
    cell::{Cell, CellStyle},
    color::Color,
    text::TextAttributes,
};

use crate::TuiPlatform;

pub struct Gauge {
    pub ratio: f64,
    pub label: Option<Rc<str>>,
    pub filled: Color,
    pub unfilled: Color,
    pub label_color: Color,
}

impl Gauge {
    pub fn new(ratio: f64) -> Self {
        Self {
            ratio: ratio.clamp(0.0, 1.0),
            label: None,
            filled: Color::GREEN,
            unfilled: Color::Reset,
            label_color: Color::Reset,
        }
    }

    pub fn label(mut self, label: Rc<str>) -> Self {
        self.label = Some(label);
        self
    }

    pub const fn filled(mut self, color: Color) -> Self {
        self.filled = color;
        self
    }

    pub const fn unfilled(mut self, color: Color) -> Self {
        self.unfilled = color;
        self
    }

    pub const fn label_color(mut self, color: Color) -> Self {
        self.label_color = color;
        self
    }
}

impl Atom<TuiPlatform> for Gauge {
    fn measure(&self, _: &mut TuiPlatform, constraints: Constraints) -> Size {
        let width = self.label.as_ref().map_or_else(
            || percentage_label(self.ratio).1,
            |label| label.chars().count(),
        );
        constraints.constrain(Size::new(width.max(1) as f32, 1.0))
    }

    fn paint(&self, platform: &mut TuiPlatform, area: LogicalRect) {
        let mut cells = platform.cells(area);
        let width = cells.columns();
        let rows = cells.rows();
        let filled = (width as f64 * self.ratio.clamp(0.0, 1.0)).round() as usize;
        for y in 0..rows {
            for x in 0..width {
                let background = if x < filled {
                    self.filled
                } else {
                    self.unfilled
                };
                cells.set_cell(
                    x,
                    y,
                    Cell::new(' ').style(CellStyle::new().background(background)),
                );
            }
        }
        if rows == 0 {
            return;
        }
        let style = |x| {
            CellStyle::new()
                .foreground(self.label_color)
                .background(if x < filled {
                    self.filled
                } else {
                    self.unfilled
                })
                .attributes(TextAttributes::BOLD)
        };
        if let Some(label) = &self.label {
            let label_width = label.chars().count().min(width);
            let start = (width - label_width) / 2;
            for (offset, character) in label.chars().take(label_width).enumerate() {
                let x = start + offset;
                cells.set_cell(x, rows / 2, Cell::new(character).style(style(x)));
            }
        } else {
            let (label, label_width) = percentage_label(self.ratio);
            let label_width = label_width.min(width);
            let start = (width - label_width) / 2;
            for (offset, character) in label.into_iter().take(label_width).enumerate() {
                let x = start + offset;
                cells.set_cell(x, rows / 2, Cell::new(character).style(style(x)));
            }
        }
    }

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        area
    }
}

fn percentage_label(ratio: f64) -> ([char; 4], usize) {
    let percentage = (ratio.clamp(0.0, 1.0) * 100.0).round() as usize;
    if percentage == 100 {
        (['1', '0', '0', '%'], 4)
    } else if percentage >= 10 {
        (
            [
                char::from(b'0' + (percentage / 10) as u8),
                char::from(b'0' + (percentage % 10) as u8),
                '%',
                ' ',
            ],
            3,
        )
    } else {
        ([char::from(b'0' + percentage as u8), '%', ' ', ' '], 2)
    }
}
