//! terminal cell drawing

use blit::{LogicalPoint, LogicalRect};
use unicode_width::UnicodeWidthChar;

use crate::{
    Cell as ScreenCell, CellText, TuiRenderer,
    color::Color,
    text::{
        HorizontalAlign, TextAttributes, TextLayoutRequest, TextOverflow, TextRequest,
        VerticalAlign,
    },
};

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
}

impl TuiRenderer {
    pub fn cells(&mut self, area: LogicalRect, clip: LogicalRect) -> CellBuffer<'_> {
        CellBuffer::new(self, area, clip)
    }

    pub fn paint_text(&mut self, request: TextRequest, clip: LogicalRect) {
        self.paint_text_at(request, clip, None);
    }
}

impl CellBuffer<'_> {
    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn clear(&mut self, cell: Cell) {
        let width = cell
            .character
            .and_then(UnicodeWidthChar::width)
            .unwrap_or(1);
        if width > 1 && cell.style.background.is_some() {
            let background = Cell {
                character: None,
                style: cell.style,
            };
            for y in 0..self.rows {
                for x in 0..self.columns {
                    self.renderer.paint_cell(
                        self.origin_x + x as isize,
                        self.origin_y + y as isize,
                        self.area,
                        self.clip,
                        background,
                    );
                }
            }
        }
        let step = width.max(1);
        for y in 0..self.rows {
            for x in (0..self.columns).step_by(step) {
                self.renderer.paint_cell(
                    self.origin_x + x as isize,
                    self.origin_y + y as isize,
                    self.area,
                    self.clip,
                    cell,
                );
            }
        }
    }

    pub fn set_cell(&mut self, x: usize, y: usize, cell: Cell) {
        if x >= self.columns || y >= self.rows {
            return;
        }
        self.renderer.paint_cell(
            self.origin_x + x as isize,
            self.origin_y + y as isize,
            self.area,
            self.clip,
            cell,
        );
    }

    pub fn write(&mut self, x: usize, y: usize, text: &str, style: CellStyle) {
        if x >= self.columns || y >= self.rows || text.is_empty() {
            return;
        }
        let Some(clip) = self.area.intersection(self.clip) else {
            return;
        };
        let area = LogicalRect::new(
            (self.origin_x + x as isize) as f32,
            (self.origin_y + y as isize) as f32,
            (self.columns - x) as f32,
            (self.rows - y) as f32,
        );
        let text = self.renderer.text_run(text);
        self.renderer.paint_text_at(
            TextRequest::new(text, area)
                .color(style.foreground_color())
                .attributes(style.text_attributes()),
            clip,
            style.background_color(),
        );
    }
}

impl TuiRenderer {
    fn paint_text_at(
        &mut self,
        request: TextRequest,
        clip: LogicalRect,
        background: Option<Color>,
    ) {
        let (area_left, area_top, area_right, area_bottom) = self.cell_bounds(request.area);
        let (clip_left, clip_top, clip_right, clip_bottom) = self.cell_bounds(clip);
        let left = area_left.max(clip_left);
        let top = area_top.max(clip_top);
        let right = area_right.min(clip_right);
        let bottom = area_bottom.min(clip_bottom);
        let layout_request = TextLayoutRequest {
            text: request.text,
            wrap: request.options.wrap,
            max_width: Some(request.area.width),
            max_lines: request.options.max_lines,
        };
        let run = self.text_run_index(request.text);
        let cell_run = u32::try_from(run).expect("too many tui text runs");
        let layout = self.layout_text(&layout_request);
        let layout = self.text_layouts.get_index(layout);
        let spans = &self.text_runs.get_index(run).spans;
        let mut span_index = 0;
        let ellipsis = request.options.overflow == TextOverflow::Ellipsis
            && (layout.truncated || layout.width as f32 > request.area.width);
        let maximum = request.area.width.floor().max(1.0) as usize;
        let area_width = area_right as isize - area_left as isize;
        let area_height = area_bottom as isize - area_top as isize;
        let line_count = layout.lines.len() as isize;
        let start_y = match request.options.vertical_align {
            VerticalAlign::Top => area_top as isize,
            VerticalAlign::Center => area_top as isize + (area_height - line_count).div_euclid(2),
            VerticalAlign::Bottom => area_bottom as isize - line_count,
        };
        for (line_index, line) in layout.lines.iter().enumerate() {
            let y = start_y + line_index as isize;
            if y < top as isize || y >= bottom as isize {
                continue;
            }
            let mut line_end = line.end;
            let mut line_width = line.width;
            let line_ellipsis = ellipsis && line_index + 1 == layout.lines.len();
            if line_ellipsis {
                while line_width >= maximum && line_end != line.start {
                    line_end -= 1;
                    line_width -= layout.graphemes[line_end].width;
                }
                line_width += 1;
            }
            let start_x = match request.options.horizontal_align {
                HorizontalAlign::Left => area_left as isize - request.offset_x.round() as isize,
                HorizontalAlign::Center => {
                    area_left as isize + (area_width - line_width as isize).div_euclid(2)
                }
                HorizontalAlign::Right => area_right as isize - line_width as isize,
            };
            let mut column = 0;
            let graphemes = layout.graphemes[line.start..line_end]
                .iter()
                .map(|grapheme| {
                    (
                        CellText::Run {
                            run: cell_run,
                            start: grapheme.start as u32,
                            end: grapheme.end as u32,
                            width: u16::try_from(grapheme.width).expect("tui grapheme is too wide"),
                        },
                        grapheme.width,
                        Some(grapheme.start),
                    )
                })
                .chain(line_ellipsis.then_some((CellText::Scalar('…'), 1, None)));
            for (grapheme, width, byte_offset) in graphemes {
                if let Some(byte_offset) = byte_offset {
                    while span_index + 1 < spans.len() && byte_offset >= spans[span_index].end {
                        span_index += 1;
                    }
                }
                let span = byte_offset.and_then(|_| spans.get(span_index));
                let color = span.and_then(|span| span.color).unwrap_or(request.color);
                let attributes = span.map_or(request.attributes, |span| {
                    request.attributes | span.attributes
                });
                let x = start_x + column as isize;
                if x >= right as isize {
                    break;
                }
                if x >= left as isize && x + width as isize <= right as isize {
                    let index = y as usize * self.columns + x as usize;
                    Self::paint_glyph(
                        &mut self.frame_cells,
                        self.columns,
                        index,
                        grapheme,
                        width,
                        CellStyle {
                            background,
                            foreground: color,
                            attributes,
                        },
                    );
                }
                column += width;
            }
        }
    }

    fn paint_cell(&mut self, x: isize, y: isize, area: LogicalRect, clip: LogicalRect, cell: Cell) {
        let glyph = cell.character().and_then(|character| {
            character
                .width()
                .filter(|width| *width != 0)
                .map(|width| (CellText::Scalar(character), width))
        });
        let width = glyph.map_or(1, |(_, width)| width);
        if x < 0 || y < 0 || y >= self.rows as isize || x + width as isize > self.columns as isize {
            return;
        }
        if width == 1 {
            let point = LogicalPoint::new(x as f32 + 0.5, y as f32 + 0.5);
            if !area.contains(point) || !clip.contains(point) {
                return;
            }
        } else if (0..width).any(|offset| {
            let point = LogicalPoint::new(x as f32 + offset as f32 + 0.5, y as f32 + 0.5);
            !area.contains(point) || !clip.contains(point)
        }) {
            return;
        }
        let index = y as usize * self.columns + x as usize;
        let style = cell.cell_style();
        if let Some((text, width)) = glyph {
            Self::paint_glyph(
                &mut self.frame_cells,
                self.columns,
                index,
                text,
                width,
                style,
            );
        } else if let Some(background) = style.background_color() {
            if self.frame_cells[index].text != CellText::Scalar(' ') {
                Self::clear_glyph(&mut self.frame_cells, self.columns, index);
            }
            self.frame_cells[index].background = background;
        }
    }

    #[inline]
    fn paint_glyph(
        frame_cells: &mut [ScreenCell],
        columns: usize,
        index: usize,
        text: CellText,
        width: usize,
        style: CellStyle,
    ) {
        for index in index..index + width {
            if frame_cells[index].text != CellText::Scalar(' ') {
                Self::clear_glyph(frame_cells, columns, index);
            }
        }
        let background = style
            .background_color()
            .unwrap_or(frame_cells[index].background);
        frame_cells[index] = ScreenCell {
            text,
            foreground: style.foreground_color(),
            background,
            attributes: style.text_attributes(),
            valid: true,
        };
        for cell in &mut frame_cells[index + 1..index + width] {
            *cell = ScreenCell {
                text: CellText::Continuation,
                foreground: style.foreground_color(),
                background,
                attributes: style.text_attributes(),
                valid: true,
            };
        }
    }

    fn clear_glyph(frame_cells: &mut [ScreenCell], columns: usize, index: usize) {
        let row = index / columns;
        let mut start = index;
        while start > row * columns && frame_cells[start].text == CellText::Continuation {
            start -= 1;
        }
        let width = match frame_cells[start].text {
            CellText::Continuation => 1,
            CellText::Scalar(character) => character.width().unwrap_or(1).max(1),
            CellText::Run { width, .. } => usize::from(width),
        };
        for cell in &mut frame_cells[start..(start + width).min((row + 1) * columns)] {
            cell.text = CellText::Scalar(' ');
            cell.foreground = Color::Reset;
            cell.attributes = TextAttributes::NONE;
        }
    }
}

impl CellBuffer<'_> {
    fn new(renderer: &mut TuiRenderer, area: LogicalRect, clip: LogicalRect) -> CellBuffer<'_> {
        CellBuffer {
            renderer,
            area,
            clip,
            origin_x: area.x.round() as isize,
            origin_y: area.y.round() as isize,
            columns: area.width.round().max(0.0) as usize,
            rows: area.height.round().max(0.0) as usize,
        }
    }
}
