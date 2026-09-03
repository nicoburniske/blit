use blit::{Atom, Constraints, LogicalRect, Size};
use blit_tui_render::{
    cell::{Cell, CellStyle},
    color::Color,
    text::{
        HorizontalAlign, TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId,
        VerticalAlign,
    },
};

use crate::TuiPlatform;

pub struct Gauge {
    pub ratio: f64,
    pub label: Option<TextRunId>,
    pub filled: Color,
    pub unfilled: Color,
}

impl Gauge {
    pub fn new(ratio: f64) -> Self {
        Self {
            ratio: ratio.clamp(0.0, 1.0),
            label: None,
            filled: Color::GREEN,
            unfilled: Color::Reset,
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
}

impl Atom<TuiPlatform> for Gauge {
    fn measure(&self, platform: &mut TuiPlatform, constraints: Constraints) -> Size {
        let width = self.label.map_or_else(
            || percentage_label(self.ratio).1,
            |label| {
                platform
                    .renderer_mut()
                    .measure_text(&TextLayoutRequest::new(label).max_lines(1))
                    .width as usize
            },
        );
        constraints.constrain(Size::new(width.max(1) as f32, 1.0))
    }

    fn paint(&self, platform: &mut TuiPlatform, area: LogicalRect) {
        let label = self.label.unwrap_or_else(|| {
            let (label, len) = percentage_label(self.ratio);
            platform
                .renderer_mut()
                .text_run(std::str::from_utf8(&label[..len]).expect("percentage label is ASCII"))
        });
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
                    Cell::new(' ').style(
                        CellStyle::new()
                            .foreground(if x < filled {
                                self.unfilled
                            } else {
                                self.filled
                            })
                            .background(background),
                    ),
                );
            }
        }
        platform.paint_text(
            TextRequest::new(label, area)
                .attributes(TextAttributes::BOLD)
                .options(
                    TextOptions::new()
                        .max_lines(1)
                        .horizontal_align(HorizontalAlign::Center)
                        .vertical_align(VerticalAlign::Center),
                ),
        );
    }

    fn measure_depends_on_constraints(&self) -> bool {
        false
    }
}

fn percentage_label(ratio: f64) -> ([u8; 4], usize) {
    let percentage = (ratio.clamp(0.0, 1.0) * 100.0).round() as usize;
    if percentage == 100 {
        (*b"100%", 4)
    } else if percentage >= 10 {
        (
            [
                b'0' + (percentage / 10) as u8,
                b'0' + (percentage % 10) as u8,
                b'%',
                b' ',
            ],
            3,
        )
    } else {
        ([b'0' + percentage as u8, b'%', b' ', b' '], 2)
    }
}
