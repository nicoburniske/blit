//! terminal cell drawing

use unicode_width::UnicodeWidthChar;

use crate::{TuiRenderer, color::Color, text::TextAttributes};
use blit::LogicalRect;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct CellStyle {
        new(),
        @optional {
            background: Color,
        },
        foreground: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
    }
}

impl CellStyle {
    pub const fn foreground_color(self) -> Color {
        self.foreground
    }

    pub const fn background_color(self) -> Option<Color> {
        self.background
    }

    pub const fn text_attributes(self) -> TextAttributes {
        self.attributes
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub character: Option<char>,
    pub style: CellStyle,
}

impl Cell {
    pub fn new(character: char) -> Self {
        Self {
            character: Some(character),
            style: CellStyle::new(),
        }
    }

    pub const fn style(mut self, style: CellStyle) -> Self {
        self.style = style;
        self
    }

    pub const fn character(self) -> Option<char> {
        self.character
    }

    pub const fn cell_style(self) -> CellStyle {
        self.style
    }
}

pub struct CellBuffer<'a> {
    renderer: &'a mut TuiRenderer,
    area: LogicalRect,
    clip: LogicalRect,
    origin_x: isize,
    origin_y: isize,
    columns: usize,
    rows: usize,
    z: u32,
}

impl CellBuffer<'_> {
    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn clear(&mut self, cell: Cell) {
        for y in 0..self.rows {
            for x in 0..self.columns {
                self.set_cell(x, y, cell);
            }
        }
    }

    pub fn set_cell(&mut self, x: usize, y: usize, cell: Cell) {
        if x >= self.columns || y >= self.rows {
            return;
        }
        if let Some(character) = cell.character {
            assert_eq!(
                character.width(),
                Some(1),
                "cell buffer characters must be one column wide"
            );
        }
        self.renderer.paint_cell(
            self.origin_x + x as isize,
            self.origin_y + y as isize,
            self.area,
            self.clip,
            self.z,
            cell,
        );
    }

    pub fn write(&mut self, x: usize, y: usize, text: &str, style: CellStyle) {
        let start_x = x;
        let mut x = x;
        let mut y = y;
        for character in text.chars() {
            if character == '\n' {
                x = start_x;
                y += 1;
                if y >= self.rows {
                    break;
                }
                continue;
            }
            assert_eq!(
                character.width(),
                Some(1),
                "cell buffer text must be one column wide"
            );
            self.set_cell(x, y, Cell::new(character).style(style));
            x += 1;
        }
    }

    pub(crate) fn new(
        renderer: &mut TuiRenderer,
        area: LogicalRect,
        clip: LogicalRect,
        z: u32,
    ) -> CellBuffer<'_> {
        CellBuffer {
            renderer,
            area,
            clip,
            origin_x: area.x.round() as isize,
            origin_y: area.y.round() as isize,
            columns: area.width.round().max(0.0) as usize,
            rows: area.height.round().max(0.0) as usize,
            z,
        }
    }
}
