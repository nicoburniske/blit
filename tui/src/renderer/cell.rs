//! terminal cell drawing

use blit::{LogicalRect, PhysicalRect};
use unicode_width::UnicodeWidthChar;

use crate::{
    geometry::cell_rect,
    renderer::{
        Cells, Glyph, TuiRenderer,
        color::Color,
        text::{HorizontalAlign, TextAttributes, TextLayoutRequest, TextOverflow, TextRequest, VerticalAlign},
    },
};

blit::builder! {
    #[const]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct CellStyle {
        new(),
        #[option]
        background: Color,
        foreground: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub character: Option<char>,
    pub style: CellStyle,
}

impl Cell {
    pub const fn new(character: char) -> Self {
        Self {
            character: Some(character),
            style: CellStyle::new(),
        }
    }

    pub const fn style(mut self, style: CellStyle) -> Self {
        self.style = style;
        self
    }
}

pub struct CellBuffer<'a> {
    renderer: &'a mut TuiRenderer,
    area: PhysicalRect,
    bounds: PhysicalRect,
}

impl TuiRenderer {
    #[inline]
    pub fn cells(&mut self, area: PhysicalRect, clip: PhysicalRect) -> CellBuffer<'_> {
        let bounds = area
            .intersection(clip)
            .and_then(|bounds| bounds.intersection(self.screen()))
            .unwrap_or_default();
        CellBuffer {
            renderer: self,
            area,
            bounds,
        }
    }

    pub fn paint_text(&mut self, request: TextRequest, clip: PhysicalRect) {
        self.paint_text_at(request, clip, None);
    }
}

impl CellBuffer<'_> {
    #[inline]
    pub fn columns(&self) -> usize {
        self.area.width.max(0) as usize
    }

    #[inline]
    pub fn rows(&self) -> usize {
        self.area.height.max(0) as usize
    }

    pub fn area(&self) -> PhysicalRect {
        self.area
    }

    /// blends rgb into existing foregrounds and backgrounds without replacing text
    ///
    /// opacity ranges from zero to 255. terminal colors use the renderer's palette.
    /// glyphs crossing the clip edges are left unchanged. Kitty images are unaffected.
    pub fn tint(&mut self, color: [u8; 3], opacity: u8) {
        if opacity == 0 {
            return;
        }
        let bounds = self.bounds;
        let palette = &self.renderer.palette;
        let cells = &mut self.renderer.frame_cells;
        let alpha = u32::from(opacity);
        for y in bounds.y..bounds.y + bounds.height {
            let row = y as usize * self.renderer.columns;
            let mut start = row + bounds.x as usize;
            let mut end = start + bounds.width as usize;
            while start < end && cells.glyph[start] == Glyph::CONTINUATION.0 {
                start += 1;
            }
            if end < row + self.renderer.columns {
                while end > start && cells.glyph[end] == Glyph::CONTINUATION.0 {
                    end -= 1;
                }
            }
            for (colors, default) in [
                (&mut cells.foreground, palette.foreground),
                (&mut cells.background, palette.background),
            ] {
                if opacity == 255 {
                    colors[start..end].fill(Color::Rgb(color[0], color[1], color[2]).packed());
                    continue;
                }
                for packed in &mut colors[start..end] {
                    let destination = match Color::from_packed(*packed) {
                        Color::Reset => default,
                        Color::Indexed(index) => palette.indexed[index as usize],
                        Color::Rgb(red, green, blue) => Some([red, green, blue]),
                    };
                    let Some(destination) = destination else {
                        continue;
                    };
                    let blended: [u8; 3] = std::array::from_fn(|channel| {
                        ((u32::from(color[channel]) * alpha + u32::from(destination[channel]) * (255 - alpha) + 127)
                            / 255) as u8
                    });
                    *packed = Color::Rgb(blended[0], blended[1], blended[2]).packed();
                }
            }
        }
    }

    pub fn clear(&mut self, cell: Cell) {
        let width = cell.character.and_then(UnicodeWidthChar::width).unwrap_or(1);
        let bounds = self.bounds;
        if bounds.width == 0 || bounds.height == 0 {
            return;
        }
        if width == 1
            && let Some(background) = cell.style.background
        {
            let character = cell.character.filter(|character| character.width() == Some(1));
            let glyph = Glyph::scalar(character.unwrap_or(' '));
            let foreground = character.map_or(Color::Reset, |_| cell.style.foreground);
            let attributes = character.map_or(TextAttributes::NONE, |_| cell.style.attributes);
            for y in bounds.y..bounds.y + bounds.height {
                let start = y as usize * self.renderer.columns + bounds.x as usize;
                let end = start + bounds.width as usize;
                if self.renderer.frame_cells.glyph[start] != Glyph::SPACE.0 {
                    TuiRenderer::clear_glyph(&mut self.renderer.frame_cells, self.renderer.columns, start);
                }
                if self.renderer.frame_cells.glyph[end - 1] != Glyph::SPACE.0 {
                    TuiRenderer::clear_glyph(&mut self.renderer.frame_cells, self.renderer.columns, end - 1);
                }
                self.renderer
                    .frame_cells
                    .fill(start..end, glyph, foreground, background, attributes);
            }
            return;
        }
        if width > 1 && cell.style.background.is_some() {
            let background = Cell {
                character: None,
                style: cell.style,
            };
            for y in 0..self.rows() {
                for x in 0..self.columns() {
                    self.renderer.paint_cell(
                        self.area.x as isize + x as isize,
                        self.area.y as isize + y as isize,
                        self.bounds,
                        background,
                    );
                }
            }
        }
        let step = width.max(1);
        for y in 0..self.rows() {
            for x in (0..self.columns()).step_by(step) {
                self.renderer.paint_cell(
                    self.area.x as isize + x as isize,
                    self.area.y as isize + y as isize,
                    self.bounds,
                    cell,
                );
            }
        }
    }

    #[inline]
    pub fn set_cell(&mut self, x: usize, y: usize, cell: Cell) {
        if x >= self.columns() || y >= self.rows() {
            return;
        }
        self.renderer.paint_cell(
            self.area.x as isize + x as isize,
            self.area.y as isize + y as isize,
            self.bounds,
            cell,
        );
    }

    pub fn write(&mut self, x: usize, y: usize, text: &str, style: CellStyle) {
        if x >= self.columns() || y >= self.rows() || text.is_empty() {
            return;
        }
        if self.bounds.width == 0 || self.bounds.height == 0 {
            return;
        }
        let area = LogicalRect::new(
            (self.area.x as isize + x as isize) as f32,
            (self.area.y as isize + y as isize) as f32,
            (self.columns() - x) as f32,
            (self.rows() - y) as f32,
        );
        let text = self.renderer.text_run(text);
        self.renderer.paint_text_at(
            TextRequest::new(text, area)
                .color(style.foreground)
                .attributes(style.attributes),
            self.bounds,
            style.background,
        );
    }
}

impl TuiRenderer {
    fn paint_text_at(&mut self, request: TextRequest, clip: PhysicalRect, background: Option<Color>) {
        let area = cell_rect(request.area);
        let bounds = area
            .intersection(clip)
            .and_then(|bounds| bounds.intersection(self.screen()))
            .unwrap_or_default();
        let area_width = area.width as usize;
        let layout_request = TextLayoutRequest {
            text: request.text,
            wrap: request.options.wrap,
            max_columns: Some(area_width),
            max_lines: request.options.max_lines,
        };
        let run = self.text_run_index(request.text);
        let cell_run = u32::try_from(run).expect("too many tui text runs");
        let layout = self.layout_text(&layout_request);
        let layout = self.text_layouts.get_index(layout);
        let spans = &self.text_runs.get_key_index(run).spans;
        let mut span_index = 0;
        let base_style = CellStyle {
            background,
            foreground: request.color,
            attributes: request.attributes,
        };
        let resolve_style = |span: &crate::renderer::ResolvedSpan| {
            let mut attributes = request.attributes | span.attributes;
            attributes.set(span.remove_attributes, false);
            CellStyle {
                background: span.background.or(background),
                foreground: span.color.unwrap_or(request.color),
                attributes,
            }
        };
        let mut span_style = spans.first().map_or(base_style, resolve_style);
        let ellipsis = request.options.overflow == TextOverflow::Ellipsis;
        let maximum = area_width.max(1);
        let area_height = area.height as isize;
        let line_count = layout.lines.len() as isize;
        let start_y = match request.options.vertical_align {
            VerticalAlign::Top => area.y as isize,
            VerticalAlign::Center => area.y as isize + (area_height - line_count).div_euclid(2),
            VerticalAlign::Bottom => (area.y + area.height) as isize - line_count,
        };
        let right = bounds.x + bounds.width;
        let bottom = bounds.y + bounds.height;
        for (line_index, line) in layout.lines.iter().enumerate() {
            let y = start_y + line_index as isize;
            if y < bounds.y as isize || y >= bottom as isize {
                continue;
            }
            let mut line_end = layout
                .lines
                .get(line_index + 1)
                .map_or(layout.graphemes.len(), |line| line.start);
            let mut line_width = line.width;
            let line_ellipsis =
                ellipsis && line_index + 1 == layout.lines.len() && (layout.truncated || line.width > maximum);
            if line_ellipsis {
                while line_width >= maximum && line_end != line.start {
                    line_end -= 1;
                    line_width -= usize::from(layout.graphemes[line_end].width);
                }
                line_width += 1;
            }
            let start_x = match request.options.horizontal_align {
                HorizontalAlign::Left => area.x as isize - request.offset_x.round() as isize,
                HorizontalAlign::Center => area.x as isize + (area_width as isize - line_width as isize).div_euclid(2),
                HorizontalAlign::Right => (area.x + area.width) as isize - line_width as isize,
            };
            let mut column = 0;
            let ellipsis_offset = layout
                .graphemes
                .get(line_end)
                .or_else(|| line_end.checked_sub(1).and_then(|end| layout.graphemes.get(end)))
                .map(|grapheme| grapheme.start as usize);
            let graphemes = layout.graphemes[line.start..line_end]
                .iter()
                .map(|grapheme| {
                    (
                        grapheme.resolve(cell_run),
                        usize::from(grapheme.width),
                        Some(grapheme.start as usize),
                    )
                })
                .chain(line_ellipsis.then_some((Glyph::scalar('…'), 1, ellipsis_offset)));
            for (grapheme, width, byte_offset) in graphemes {
                let style = if let Some(byte_offset) = byte_offset {
                    let previous = span_index;
                    while span_index + 1 < spans.len() && byte_offset >= spans[span_index].end {
                        span_index += 1;
                    }
                    if span_index != previous {
                        span_style = spans.get(span_index).map_or(base_style, resolve_style);
                    }
                    span_style
                } else {
                    base_style
                };
                let x = start_x + column as isize;
                if x >= right as isize {
                    break;
                }
                if x >= bounds.x as isize && x + width as isize <= right as isize {
                    let index = y as usize * self.columns + x as usize;
                    Self::paint_glyph(&mut self.frame_cells, self.columns, index, grapheme, width, style);
                }
                column += width;
            }
        }
    }

    fn paint_cell(&mut self, x: isize, y: isize, bounds: PhysicalRect, cell: Cell) {
        let glyph = cell.character.and_then(|character| {
            character
                .width()
                .filter(|width| *width != 0)
                .map(|width| (Glyph::scalar(character), width))
        });
        let width = glyph.map_or(1, |(_, width)| width);
        if x < bounds.x as isize
            || y < bounds.y as isize
            || y >= (bounds.y + bounds.height) as isize
            || x + width as isize > (bounds.x + bounds.width) as isize
        {
            return;
        }
        let index = y as usize * self.columns + x as usize;
        let style = cell.style;
        if let Some((text, width)) = glyph {
            Self::paint_glyph(&mut self.frame_cells, self.columns, index, text, width, style);
        } else if let Some(background) = style.background {
            if self.frame_cells.glyph[index] != Glyph::SPACE.0 {
                Self::clear_glyph(&mut self.frame_cells, self.columns, index);
            }
            self.frame_cells.background[index] = background.packed();
        }
    }

    #[inline]
    fn paint_glyph(
        frame_cells: &mut Cells,
        columns: usize,
        index: usize,
        glyph: Glyph,
        width: usize,
        style: CellStyle,
    ) {
        for index in index..index + width {
            if frame_cells.glyph[index] != Glyph::SPACE.0 {
                Self::clear_glyph(frame_cells, columns, index);
            }
        }
        let background = style.background.map_or(frame_cells.background[index], Color::packed);
        let foreground = style.foreground.packed();
        let attributes = style.attributes.0;
        frame_cells.glyph[index] = glyph.0;
        frame_cells.foreground[index] = foreground;
        frame_cells.background[index] = background;
        frame_cells.attributes[index] = attributes;
        let continuation = index + 1..index + width;
        frame_cells.glyph[continuation.clone()].fill(Glyph::CONTINUATION.0);
        frame_cells.foreground[continuation.clone()].fill(foreground);
        frame_cells.background[continuation.clone()].fill(background);
        frame_cells.attributes[continuation].fill(attributes);
    }

    fn clear_glyph(frame_cells: &mut Cells, columns: usize, index: usize) {
        let row = index / columns;
        let mut start = index;
        while start > row * columns && frame_cells.glyph[start] == Glyph::CONTINUATION.0 {
            start -= 1;
        }
        let mut end = start + 1;
        while end < (row + 1) * columns && frame_cells.glyph[end] == Glyph::CONTINUATION.0 {
            end += 1;
        }
        let range = start..end;
        frame_cells.glyph[range.clone()].fill(Glyph::SPACE.0);
        frame_cells.foreground[range.clone()].fill(Color::Reset.packed());
        frame_cells.attributes[range].fill(TextAttributes::NONE.0);
    }
}
