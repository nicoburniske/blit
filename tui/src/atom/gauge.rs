use blit::{Atom, AtomCx, Constraints, PhysicalRect, Size};

use crate::{
    Context,
    cell::{Cell, CellStyle},
    color::Color,
    text::{HorizontalAlign, TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId},
};

pub struct Gauge {
    pub ratio: f64,
    pub label: Option<TextRunId>,
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

    pub const fn label(mut self, label: TextRunId) -> Self {
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

blit::atom_widget!(Gauge);

impl<C: Context> Atom<C> for Gauge {
    fn measure(&self, cx: &mut AtomCx<'_, C>, constraints: Constraints<i32>) -> Size<i32> {
        let width = self.label.map_or_else(
            || percentage_label(self.ratio).1 as i32,
            |label| {
                cx.context_mut()
                    .as_mut()
                    .renderer_mut()
                    .measure_text(&TextLayoutRequest::new(label).max_lines(1))
                    .width
            },
        );
        constraints.constrain(Size::new(width.max(1), 1))
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: PhysicalRect) {
        {
            let mut cells = cx.context_mut().as_mut().cells(area);
            let width = cells.columns();
            let rows = cells.rows();
            let filled = (width as f64 * self.ratio.clamp(0.0, 1.0)).round() as usize;
            for y in 0..rows {
                for x in 0..width {
                    let background = if x < filled { self.filled } else { self.unfilled };
                    cells.set_cell(x, y, Cell::new(' ').style(CellStyle::new().background(background)));
                }
            }
            if rows == 0 {
                return;
            }
            let style = |x| {
                CellStyle::new()
                    .foreground(self.label_color)
                    .background(if x < filled { self.filled } else { self.unfilled })
                    .attributes(TextAttributes::BOLD)
            };
            if self.label.is_none() {
                let (label, label_width) = percentage_label(self.ratio);
                let label_width = label_width.min(width);
                let start = (width - label_width) / 2;
                for (offset, character) in label.into_iter().take(label_width).enumerate() {
                    let x = start + offset;
                    cells.set_cell(x, rows / 2, Cell::new(character).style(style(x)));
                }
            }
        }
        if let Some(label) = self.label {
            cx.context_mut().as_mut().paint_text(
                TextRequest::new(
                    label,
                    PhysicalRect::new(area.x, area.y + area.height / 2, area.width, 1),
                )
                .color(self.label_color)
                .attributes(TextAttributes::BOLD)
                .options(
                    TextOptions::new()
                        .max_lines(1)
                        .horizontal_align(HorizontalAlign::Center),
                ),
            );
        }
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
