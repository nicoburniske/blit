use std::fmt::Write as _;

use base64::Engine as _;
use blit::{LogicalPoint, LogicalRect};

use crate::{
    BASE64, Background, Cell, CellText, Glyph, KittyPlacement, TuiRenderer,
    color::Color,
    image::ImagePlacement,
    surface::{Cell as SurfaceCell, CellBuffer},
    text::{
        HorizontalAlign, TextAttributes, TextLayoutRequest, TextOverflow, TextRequest,
        VerticalAlign,
    },
    write_color,
};

impl TuiRenderer {
    pub fn begin_frame(&mut self) {
        self.output.clear();
        self.backgrounds.fill(Background::default());
        self.glyphs.fill(None);
        self.kitty_placements.clear();
        self.z = 0;
    }

    pub fn cells(&mut self, area: LogicalRect, clip: LogicalRect) -> CellBuffer<'_> {
        let z = self.next_z();
        CellBuffer::new(self, area, clip, z)
    }

    pub fn paint_text(&mut self, request: TextRequest, clip: LogicalRect) {
        let z = self.next_z();
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
                    self.glyphs[index] = Some(Glyph {
                        text: grapheme,
                        color,
                        attributes,
                        z,
                    });
                    for continuation in 1..width {
                        self.glyphs[index + continuation] = Some(Glyph {
                            text: CellText::Empty,
                            color,
                            attributes,
                            z,
                        });
                    }
                }
                column += width;
            }
        }
    }

    pub fn place_image(&mut self, request: ImagePlacement, clip: LogicalRect) {
        let z = self.next_z();
        if let Some(area) = request.area.intersection(clip) {
            let x = area.x.floor().max(0.0) as usize;
            let y = area.y.floor().max(0.0) as usize;
            let right = (area.x + area.width).ceil().min(self.columns as f32) as usize;
            let bottom = (area.y + area.height).ceil().min(self.rows as f32) as usize;
            if right > x && bottom > y {
                self.kitty_placements.push(KittyPlacement {
                    id: z.checked_add(1).expect("too many tui image placements"),
                    image: request.image.0 as u32,
                    x,
                    y,
                    width: right - x,
                    height: bottom - y,
                });
            }
        }
    }

    pub(crate) fn paint_cell(
        &mut self,
        x: isize,
        y: isize,
        area: LogicalRect,
        clip: LogicalRect,
        z: u32,
        cell: SurfaceCell,
    ) {
        if x < 0 || y < 0 || x >= self.columns as isize || y >= self.rows as isize {
            return;
        }
        let point = LogicalPoint::new(x as f32 + 0.5, y as f32 + 0.5);
        if !area.contains(point) || !clip.contains(point) {
            return;
        }
        let index = y as usize * self.columns + x as usize;
        let style = cell.cell_style();
        if let Some(background) = style.background_color() {
            self.backgrounds[index] = Background {
                color: background,
                z,
            };
        }
        if let Some(character) = cell.character() {
            self.glyphs[index] = Some(Glyph {
                text: CellText::Scalar(character),
                color: style.foreground_color(),
                attributes: style.text_attributes(),
                z,
            });
        }
    }

    pub fn end_frame(&mut self) {
        for index in 0..self.cells.len() {
            let background = self.backgrounds[index];
            let glyph = self.glyphs[index];
            let cell = if let Some(glyph) = glyph
                && background.z <= glyph.z
            {
                Cell {
                    text: glyph.text,
                    foreground: glyph.color,
                    background: background.color,
                    attributes: glyph.attributes,
                    valid: true,
                }
            } else {
                Cell {
                    text: CellText::Scalar(' '),
                    foreground: Color::Reset,
                    background: background.color,
                    attributes: TextAttributes::NONE,
                    valid: true,
                }
            };
            self.changed[index] = self.cells[index] != cell;
            self.set_cell(index, cell);
        }

        let mut style = None;
        for y in 0..self.rows {
            let mut x = 0;
            while x < self.columns {
                let index = y * self.columns + x;
                if !self.changed[index] {
                    x += 1;
                    continue;
                }
                write!(self.output, "\x1b[{};{}H", y + 1, x + 1).unwrap();
                while x < self.columns {
                    let index = y * self.columns + x;
                    let cell = &self.cells[index];
                    if !self.changed[index] {
                        break;
                    }
                    let next_style = (cell.foreground, cell.background, cell.attributes);
                    if style != Some(next_style) {
                        if let Some((foreground, background, attributes)) = style
                            && attributes == cell.attributes
                        {
                            self.output.push_str("\x1b[");
                            let mut separator = false;
                            if foreground != cell.foreground {
                                write_color(&mut self.output, cell.foreground, true);
                                separator = true;
                            }
                            if background != cell.background {
                                if separator {
                                    self.output.push(';');
                                }
                                write_color(&mut self.output, cell.background, false);
                            }
                        } else {
                            self.output.push_str("\x1b[0");
                            if cell.attributes.contains(TextAttributes::BOLD) {
                                self.output.push_str(";1");
                            }
                            if cell.attributes.contains(TextAttributes::DIM) {
                                self.output.push_str(";2");
                            }
                            if cell.attributes.contains(TextAttributes::ITALIC) {
                                self.output.push_str(";3");
                            }
                            if cell.attributes.contains(TextAttributes::UNDERLINE) {
                                self.output.push_str(";4");
                            }
                            if cell.attributes.contains(TextAttributes::BLINK) {
                                self.output.push_str(";5");
                            }
                            if cell.attributes.contains(TextAttributes::INVERSE) {
                                self.output.push_str(";7");
                            }
                            if cell.attributes.contains(TextAttributes::HIDDEN) {
                                self.output.push_str(";8");
                            }
                            if cell.attributes.contains(TextAttributes::STRIKETHROUGH) {
                                self.output.push_str(";9");
                            }
                            self.output.push(';');
                            write_color(&mut self.output, cell.foreground, true);
                            self.output.push(';');
                            write_color(&mut self.output, cell.background, false);
                        }
                        self.output.push('m');
                        style = Some(next_style);
                    }
                    Self::push_cell_text(&self.text_runs, cell.text, &mut self.output);
                    x += 1;
                }
            }
        }
        if style.is_some() {
            self.output.push_str("\x1b[0m");
        }

        for placement in &self.presented_kitty_placements {
            if !self.kitty_placements.contains(placement) {
                write!(
                    self.output,
                    "\x1b_Ga=d,d=i,i={},p={},q=2\x1b\\",
                    placement.image, placement.id
                )
                .unwrap();
            }
        }
        for placement in &self.kitty_placements {
            let image = self
                .images
                .iter_mut()
                .find(|image| image.handle.id().0 == u64::from(placement.image))
                .expect("invalid terminal image");
            if !image.transmitted {
                for (index, chunk) in image.rgba.chunks(3072).enumerate() {
                    let more = usize::from((index + 1) * 3072 < image.rgba.len());
                    if index == 0 {
                        write!(
                            self.output,
                            "\x1b_Ga=t,f=32,s={},v={},i={},m={more},q=2;",
                            image.width, image.height, placement.image
                        )
                        .unwrap();
                    } else {
                        write!(self.output, "\x1b_Gm={more},q=2;").unwrap();
                    }
                    BASE64.encode_string(chunk, &mut self.output);
                    self.output.push_str("\x1b\\");
                }
                image.transmitted = true;
            }
            if !self.presented_kitty_placements.contains(placement) {
                write!(
                    self.output,
                    "\x1b[{};{}H\x1b_Ga=p,i={},p={},c={},r={},C=1,z=1,q=2\x1b\\",
                    placement.y + 1,
                    placement.x + 1,
                    placement.image,
                    placement.id,
                    placement.width,
                    placement.height,
                )
                .unwrap();
            }
        }
        self.presented_kitty_placements
            .clone_from(&self.kitty_placements);
        let mut image = 0;
        while image < self.images.len() {
            let id = self.images[image].handle.id().0 as u32;
            if !self.images[image].handle.is_uniquely_owned()
                || self
                    .kitty_placements
                    .iter()
                    .any(|placement| placement.image == id)
            {
                image += 1;
                continue;
            }
            if self.images[image].transmitted {
                write!(self.output, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\").unwrap();
            }
            self.images.swap_remove(image);
            self.presented_kitty_placements
                .retain(|placement| placement.image != id);
        }
        self.text_layouts.trim_to_weight();
        self.text_runs
            .trim_to_weight_if(|_, run| run.screen_references == 0);
    }

    pub fn invalidate(&mut self) {
        for cell in &mut self.cells {
            cell.valid = false;
        }
    }

    fn next_z(&mut self) -> u32 {
        self.z = self.z.checked_add(1).expect("too many tui draw operations");
        self.z
    }
}
