use std::{
    collections::hash_map::DefaultHasher,
    fmt::Write as _,
    hash::{Hash, Hasher},
    io,
    mem::size_of,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use blit::{
    color::Color,
    command_list::{Command, CommandList},
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect, Scale2},
    image::{ImageData, ImageHandle, ImageId},
    renderer::{RenderGeometry, Renderer},
    style::Border,
    text::{
        HorizontalAlign, TextLayoutRequest, TextOverflow, TextRequest, TextRunId, TextStyle,
        TextWrap, VerticalAlign,
    },
};
use blit_cache::{DeferredCache, Scale};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const TEXT_RUN_CACHE_CAPACITY: usize = 2 * 1024 * 1024;
const TEXT_LAYOUT_CACHE_CAPACITY: usize = 4 * 1024 * 1024;

pub const CELL_NATIVE: LogicalSize = LogicalSize {
    width: 1.0,
    height: 1.0,
};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct RendererConfig {
        new(),
        columns: u16 = 80,
        rows: u16 = 24,
        cell_size: LogicalSize = CELL_NATIVE,
    }
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl RendererConfig {
    fn validate(&self) {
        assert!(self.cell_size.width.is_finite() && self.cell_size.width > 0.0);
        assert!(self.cell_size.height.is_finite() && self.cell_size.height > 0.0);
    }
}

pub struct TerminalRenderer {
    columns: usize,
    rows: usize,
    cell_size: LogicalSize,
    scale: Scale2,
    text_runs: DeferredCache<RunKey, CachedRun, RunScale>,
    text_layouts: DeferredCache<LayoutKey, TextLayout, LayoutScale>,
    next_text_run: u32,
    layout_lines: Vec<Line>,
    layout_graphemes: Vec<LayoutGrapheme>,
    images: Vec<StoredImage>,
    kitty_placements: Vec<KittyPlacement>,
    presented_kitty_placements: Vec<KittyPlacement>,
    pixels: Vec<Pixel>,
    glyphs: Vec<Option<Glyph>>,
    boxes: Vec<BoxCell>,
    cells: Vec<Cell>,
    previous: Vec<Cell>,
    damaged: Vec<bool>,
    output: String,
}

impl TerminalRenderer {
    pub fn new(config: RendererConfig) -> Self {
        config.validate();
        let RendererConfig {
            columns,
            rows,
            cell_size,
        } = config;
        let columns = usize::from(columns.max(1));
        let rows = usize::from(rows.max(1));
        Self {
            columns,
            rows,
            cell_size,
            scale: Scale2 {
                x: 1.0 / cell_size.width,
                y: 1.0 / cell_size.height,
            },
            text_runs: DeferredCache::new(RunScale, TEXT_RUN_CACHE_CAPACITY),
            text_layouts: DeferredCache::new(LayoutScale, TEXT_LAYOUT_CACHE_CAPACITY),
            next_text_run: 1,
            layout_lines: Vec::new(),
            layout_graphemes: Vec::new(),
            images: Vec::new(),
            kitty_placements: Vec::new(),
            presented_kitty_placements: Vec::new(),
            pixels: vec![Pixel::default(); columns * 2 * rows * 2],
            glyphs: vec![None; columns * rows],
            boxes: vec![BoxCell::default(); columns * rows],
            cells: vec![Cell::default(); columns * rows],
            previous: vec![Cell::invalid(); columns * rows],
            damaged: vec![true; columns * rows],
            output: String::new(),
        }
    }

    pub fn screen(&self) -> PhysicalRect {
        PhysicalRect {
            x: 0,
            y: 0,
            width: self.columns as i32,
            height: self.rows as i32,
        }
    }

    pub fn resize(&mut self, config: RendererConfig) {
        config.validate();
        let RendererConfig {
            columns,
            rows,
            cell_size,
        } = config;
        let columns = usize::from(columns.max(1));
        let rows = usize::from(rows.max(1));
        self.cell_size = cell_size;
        self.scale = Scale2 {
            x: 1.0 / cell_size.width,
            y: 1.0 / cell_size.height,
        };
        if self.columns == columns && self.rows == rows {
            return;
        }
        self.columns = columns;
        self.rows = rows;
        self.pixels.resize(columns * 2 * rows * 2, Pixel::default());
        self.glyphs.resize(columns * rows, None);
        self.boxes.resize(columns * rows, BoxCell::default());
        self.cells.resize(columns * rows, Cell::default());
        self.previous = vec![Cell::invalid(); columns * rows];
        self.damaged = vec![true; columns * rows];
    }

    fn cell_bounds(&self, area: LogicalRect) -> (usize, usize, usize, usize) {
        (
            (area.x * self.scale.x)
                .round()
                .clamp(0.0, self.columns as f32) as usize,
            (area.y * self.scale.y).round().clamp(0.0, self.rows as f32) as usize,
            ((area.x + area.width) * self.scale.x)
                .round()
                .clamp(0.0, self.columns as f32) as usize,
            ((area.y + area.height) * self.scale.y)
                .round()
                .clamp(0.0, self.rows as f32) as usize,
        )
    }

    fn clear_damaged(&mut self, bounds: (usize, usize, usize, usize)) {
        let (left, top, right, bottom) = bounds;
        let stride = self.columns * 2;
        for y in top..bottom {
            for x in left..right {
                let index = y * self.columns + x;
                if !self.damaged[index] {
                    continue;
                }
                self.glyphs[index] = None;
                self.boxes[index] = BoxCell::default();
                for sub_y in y * 2..y * 2 + 2 {
                    for sub_x in x * 2..x * 2 + 2 {
                        self.pixels[sub_y * stride + sub_x] = Pixel::default();
                    }
                }
            }
        }
    }

    pub fn output(&self) -> &[u8] {
        self.output.as_bytes()
    }

    pub fn clear_kitty_graphics(&mut self, output: &mut impl io::Write) -> io::Result<()> {
        for (index, image) in self.images.iter_mut().enumerate() {
            if image.transmitted {
                write!(output, "\x1b_Ga=d,d=I,i={},q=2\x1b\\", index + 1)?;
                image.transmitted = false;
            }
        }
        self.presented_kitty_placements.clear();
        output.flush()?;
        Ok(())
    }

    pub fn plain_text(&self) -> String {
        let mut output = String::with_capacity((self.columns + 1) * self.rows);
        for row in self.cells.chunks(self.columns) {
            for cell in row {
                output.push_str(cell.text.as_str());
            }
            while output.ends_with(' ') {
                output.pop();
            }
            output.push('\n');
        }
        output
    }

    fn text_run_index(&self, id: TextRunId) -> usize {
        let index = (id.0 as u32)
            .checked_sub(1)
            .expect("invalid terminal text run") as usize;
        assert_eq!(self.text_runs.get_index(index).id, id, "expired text run");
        index
    }

    fn layout_text(&mut self, request: &TextLayoutRequest) -> usize {
        fn start_line(lines: &mut Vec<Line>, grapheme: usize, limit: usize) -> bool {
            if lines.len() >= limit {
                return false;
            }
            lines.push(Line {
                start: grapheme,
                end: grapheme,
                width: 0,
            });
            true
        }

        let max_columns = request
            .max_width
            .map(|width| (width * self.scale.x).floor().max(0.0) as usize);
        let max_lines = usize::from(request.max_lines.unwrap_or(u16::MAX)).max(1);
        let key = LayoutKey {
            text: request.text,
            max_columns,
            max_lines,
            wrap: request.wrap,
        };
        let run = self.text_run_index(request.text);
        let text = &self.text_runs.get_index(run).text;
        let lines = &mut self.layout_lines;
        let graphemes = &mut self.layout_graphemes;
        let (_, index) = self.text_layouts.get_or_insert(key, || {
            lines.clear();
            graphemes.clear();
            start_line(lines, 0, max_lines);
            let mut truncated = false;
            match request.wrap {
                TextWrap::Word => {
                    'tokens: for (token_offset, token) in text.split_word_bound_indices() {
                        let whitespace = token.chars().all(char::is_whitespace);
                        let token_width = UnicodeWidthStr::width(token);
                        if !whitespace
                            && lines.last().unwrap().width != 0
                            && max_columns.is_some_and(|maximum| {
                                lines.last().unwrap().width + token_width > maximum
                            })
                        {
                            let current = lines.last_mut().unwrap();
                            while current.end != current.start
                                && text
                                    [graphemes.last().unwrap().start..graphemes.last().unwrap().end]
                                    .chars()
                                    .all(char::is_whitespace)
                            {
                                let grapheme = graphemes.pop().unwrap();
                                current.end -= 1;
                                current.width -= grapheme.width;
                            }
                            if !start_line(lines, graphemes.len(), max_lines) {
                                truncated = true;
                                break;
                            }
                        }
                        for (offset, grapheme) in token.grapheme_indices(true) {
                            if grapheme == "\n" || grapheme == "\r\n" {
                                if !start_line(lines, graphemes.len(), max_lines) {
                                    truncated = true;
                                    break 'tokens;
                                }
                                continue;
                            }
                            let width = UnicodeWidthStr::width(grapheme).max(1);
                            if max_columns.is_some_and(|maximum| {
                                lines.last().unwrap().width + width > maximum
                            }) && lines.last().unwrap().width != 0
                            {
                                if !start_line(lines, graphemes.len(), max_lines) {
                                    truncated = true;
                                    break 'tokens;
                                }
                                if whitespace {
                                    continue;
                                }
                            }
                            if whitespace && lines.last().unwrap().width == 0 {
                                continue;
                            }
                            let start = token_offset + offset;
                            graphemes.push(LayoutGrapheme {
                                start,
                                end: start + grapheme.len(),
                                width,
                            });
                            let current = lines.last_mut().unwrap();
                            current.end += 1;
                            current.width += width;
                        }
                    }
                }
                TextWrap::None | TextWrap::Character => {
                    'graphemes: for (start, grapheme) in text.grapheme_indices(true) {
                        if grapheme == "\n" || grapheme == "\r\n" {
                            if !start_line(lines, graphemes.len(), max_lines) {
                                truncated = true;
                                break;
                            }
                            continue;
                        }
                        let width = UnicodeWidthStr::width(grapheme).max(1);
                        if request.wrap == TextWrap::Character
                            && max_columns.is_some_and(|maximum| {
                                lines.last().unwrap().width + width > maximum
                            })
                            && lines.last().unwrap().width != 0
                            && !start_line(lines, graphemes.len(), max_lines)
                        {
                            truncated = true;
                            break 'graphemes;
                        }
                        graphemes.push(LayoutGrapheme {
                            start,
                            end: start + grapheme.len(),
                            width,
                        });
                        let current = lines.last_mut().unwrap();
                        current.end += 1;
                        current.width += width;
                    }
                }
            }
            TextLayout {
                width: lines.iter().map(|line| line.width).max().unwrap_or(0),
                lines: lines.as_slice().into(),
                graphemes: graphemes.as_slice().into(),
                truncated,
            }
        });
        index
    }
}

impl Renderer for TerminalRenderer {
    fn geometry(&self) -> RenderGeometry {
        RenderGeometry {
            physical_bounds: self.screen(),
            physical_per_logical: self.scale,
            layout_resolution: blit::layout::LayoutResolution::Discrete {
                step: self.cell_size,
            },
            supports_zoom: false,
        }
    }

    fn interaction_area(&self, area: LogicalRect, clip: LogicalRect) -> Option<LogicalRect> {
        let cell_width = self.cell_size.width;
        let cell_height = self.cell_size.height;
        let (mut left, mut top, mut right, mut bottom) = self.cell_bounds(area);
        left = left.max(
            (clip.x * self.scale.x - 0.5)
                .ceil()
                .clamp(0.0, self.columns as f32) as usize,
        );
        top = top.max(
            (clip.y * self.scale.y - 0.5)
                .ceil()
                .clamp(0.0, self.rows as f32) as usize,
        );
        right = right.min(
            ((clip.x + clip.width) * self.scale.x - 0.5)
                .ceil()
                .clamp(0.0, self.columns as f32) as usize,
        );
        bottom = bottom.min(
            ((clip.y + clip.height) * self.scale.y - 0.5)
                .ceil()
                .clamp(0.0, self.rows as f32) as usize,
        );
        (right > left && bottom > top).then(|| LogicalRect {
            x: left as f32 * cell_width,
            y: top as f32 * cell_height,
            width: (right - left) as f32 * cell_width,
            height: (bottom - top) as f32 * cell_height,
        })
    }

    fn render(&mut self, commands: &CommandList, damage: &[PhysicalRect]) {
        self.output.clear();
        self.damaged.fill(false);
        let screen = self.screen();
        let mut damage_bounds = (self.columns, self.rows, 0, 0);
        for damage in damage {
            let Some(damage) = damage.intersection(screen) else {
                continue;
            };
            let left = damage.x as usize;
            let top = damage.y as usize;
            let right = damage.x.saturating_add(damage.width) as usize;
            let bottom = damage.y.saturating_add(damage.height) as usize;
            damage_bounds.0 = damage_bounds.0.min(left);
            damage_bounds.1 = damage_bounds.1.min(top);
            damage_bounds.2 = damage_bounds.2.max(right);
            damage_bounds.3 = damage_bounds.3.max(bottom);
            for row in
                self.damaged[top * self.columns..bottom * self.columns].chunks_mut(self.columns)
            {
                row[left..right].fill(true);
            }
        }
        if damage_bounds.2 == 0 || damage_bounds.3 == 0 {
            self.text_layouts.trim_to_weight();
            self.text_runs.trim_to_weight();
            return;
        }
        self.clear_damaged(damage_bounds);
        self.kitty_placements.clear();
        let logical_screen = screen.to_logical(self.scale);
        for (z, record) in commands.iter().enumerate() {
            if !matches!(record.command, Command::Image(_)) {
                let Some(bounds) = record.bounds.intersection(screen) else {
                    continue;
                };
                let left = bounds.x as usize;
                let top = bounds.y as usize;
                let right = bounds.x.saturating_add(bounds.width) as usize;
                let bottom = bounds.y.saturating_add(bounds.height) as usize;
                if right <= damage_bounds.0
                    || bottom <= damage_bounds.1
                    || left >= damage_bounds.2
                    || top >= damage_bounds.3
                {
                    continue;
                }
            }
            let mut clip = logical_screen;
            let mut clip_id = record.clip;
            while let Some(node) = commands.clip(clip_id) {
                let Some(intersection) = clip.intersection(node.area) else {
                    clip = LogicalRect::default();
                    break;
                };
                clip = intersection;
                clip_id = node.parent;
            }
            match record.command {
                Command::Clear => {
                    self.clear_damaged(damage_bounds);
                    self.kitty_placements.clear();
                }
                Command::Rectangle(rectangle) => {
                    let cell_width = self.cell_size.width;
                    let cell_height = self.cell_size.height;
                    let (left, top, right, bottom) = self.cell_bounds(rectangle.area);
                    if rectangle.background.alpha != 0 && rectangle.opacity > 0.0 {
                        let stride = self.columns * 2;
                        for y in top.max(damage_bounds.1)..bottom.min(damage_bounds.3) {
                            for x in left.max(damage_bounds.0)..right.min(damage_bounds.2) {
                                if !self.damaged[y * self.columns + x] {
                                    continue;
                                }
                                let center_x = (x as f32 + 0.5) * cell_width;
                                let center_y = (y as f32 + 0.5) * cell_height;
                                if !clip.contains(center_x, center_y) {
                                    continue;
                                }
                                for sub_y in y * 2..y * 2 + 2 {
                                    for sub_x in x * 2..x * 2 + 2 {
                                        let pixel = &mut self.pixels[sub_y * stride + sub_x];
                                        pixel.color = blend(
                                            rectangle.background,
                                            pixel.color,
                                            rectangle.opacity,
                                        );
                                        pixel.z = z;
                                    }
                                }
                            }
                        }
                    }
                    if let Border::Solid { color, .. } = rectangle.border
                        && right > left + 1
                        && bottom > top + 1
                    {
                        let right = right - 1;
                        let bottom = bottom - 1;
                        for x in left..=right {
                            for (y, edges, rounded) in [
                                (
                                    top,
                                    if x == left {
                                        2 | 4
                                    } else if x == right {
                                        4 | 8
                                    } else {
                                        2 | 8
                                    },
                                    if x == left {
                                        rectangle.radius.top_left != 0.0
                                    } else if x == right {
                                        rectangle.radius.top_right != 0.0
                                    } else {
                                        false
                                    },
                                ),
                                (
                                    bottom,
                                    if x == left {
                                        1 | 2
                                    } else if x == right {
                                        1 | 8
                                    } else {
                                        2 | 8
                                    },
                                    if x == left {
                                        rectangle.radius.bottom_left != 0.0
                                    } else if x == right {
                                        rectangle.radius.bottom_right != 0.0
                                    } else {
                                        false
                                    },
                                ),
                            ] {
                                let center_x = (x as f32 + 0.5) * cell_width;
                                let center_y = (y as f32 + 0.5) * cell_height;
                                if self.damaged[y * self.columns + x]
                                    && clip.contains(center_x, center_y)
                                {
                                    let cell = &mut self.boxes[y * self.columns + x];
                                    cell.paint(edges, rounded, color, z);
                                }
                            }
                        }
                        for y in top + 1..bottom {
                            for x in [left, right] {
                                let center_x = (x as f32 + 0.5) * cell_width;
                                let center_y = (y as f32 + 0.5) * cell_height;
                                if self.damaged[y * self.columns + x]
                                    && clip.contains(center_x, center_y)
                                {
                                    let cell = &mut self.boxes[y * self.columns + x];
                                    cell.paint(1 | 4, false, color, z);
                                }
                            }
                        }
                    }
                }
                Command::BoxShadow(_) => {}
                Command::Text(request) => {
                    let (area_left, area_top, area_right, area_bottom) =
                        self.cell_bounds(request.area);
                    let (clip_left, clip_top, clip_right, clip_bottom) = self.cell_bounds(clip);
                    let left = area_left.max(clip_left);
                    let top = area_top.max(clip_top);
                    let right = area_right.min(clip_right);
                    let bottom = area_bottom.min(clip_bottom);
                    let layout_request = TextLayoutRequest {
                        text: request.text,
                        style: request.style,
                        wrap: request.options.wrap,
                        max_width: Some(request.area.width),
                        max_lines: request.options.max_lines,
                    };
                    let layout = self.layout_text(&layout_request);
                    let layout = self.text_layouts.get_index(layout);
                    let ellipsis = request.options.overflow == TextOverflow::Ellipsis
                        && (layout.truncated
                            || layout.width as f32 * self.cell_size.width > request.area.width);
                    let maximum = (request.area.width * self.scale.x).floor().max(1.0) as usize;
                    let area_width = area_right as isize - area_left as isize;
                    let area_height = area_bottom as isize - area_top as isize;
                    let line_count = layout.lines.len() as isize;
                    let start_y = match request.options.vertical_align {
                        VerticalAlign::Top => area_top as isize,
                        VerticalAlign::Center => {
                            area_top as isize + (area_height - line_count).div_euclid(2)
                        }
                        VerticalAlign::Bottom => area_bottom as isize - line_count,
                    };
                    for (line_index, line) in layout.lines.iter().enumerate() {
                        let mut line_end = line.end;
                        let mut line_width = line.width;
                        let line_ellipsis = ellipsis && line_index == 0;
                        if line_ellipsis {
                            while line_width >= maximum && line_end != line.start {
                                line_end -= 1;
                                line_width -= layout.graphemes[line_end].width;
                            }
                            line_width += 1;
                        }
                        let start_x = match request.options.horizontal_align {
                            HorizontalAlign::Left => {
                                area_left as isize
                                    - (request.offset_x * self.scale.x).round() as isize
                            }
                            HorizontalAlign::Center => {
                                area_left as isize
                                    + (area_width - line_width as isize).div_euclid(2)
                            }
                            HorizontalAlign::Right => area_right as isize - line_width as isize,
                        };
                        let mut column = 0;
                        let graphemes = layout.graphemes[line.start..line_end]
                            .iter()
                            .map(|grapheme| {
                                (
                                    GlyphText::Run {
                                        text: request.text,
                                        start: grapheme.start,
                                        end: grapheme.end,
                                    },
                                    grapheme.width,
                                )
                            })
                            .chain(line_ellipsis.then_some((GlyphText::Static("…"), 1)));
                        for (grapheme, width) in graphemes {
                            let x = start_x + column as isize;
                            let y = start_y + line_index as isize;
                            if x >= 0
                                && y >= 0
                                && (x as usize) >= left
                                && (y as usize) >= top
                                && (x as usize + width) <= right
                                && (y as usize) < bottom
                            {
                                let index = y as usize * self.columns + x as usize;
                                if self.damaged[index] {
                                    self.glyphs[index] = Some(Glyph {
                                        text: grapheme,
                                        color: request.color,
                                        bold: request.style.weight >= 600,
                                        z,
                                    });
                                }
                                for continuation in 1..width {
                                    let continuation_x = x as usize + continuation;
                                    let index = y as usize * self.columns + continuation_x;
                                    if self.damaged[index] {
                                        self.glyphs[index] = Some(Glyph {
                                            text: GlyphText::Static(""),
                                            color: request.color,
                                            bold: request.style.weight >= 600,
                                            z,
                                        });
                                    }
                                }
                            }
                            column += width;
                        }
                    }
                }
                Command::Image(request) => {
                    if let Some(area) = request.area.intersection(clip) {
                        let x = (area.x * self.scale.x).floor().max(0.0) as usize;
                        let y = (area.y * self.scale.y).floor().max(0.0) as usize;
                        let right = ((area.x + area.width) * self.scale.x)
                            .ceil()
                            .min(self.columns as f32) as usize;
                        let bottom = ((area.y + area.height) * self.scale.y)
                            .ceil()
                            .min(self.rows as f32) as usize;
                        if right > x && bottom > y {
                            self.kitty_placements.push(KittyPlacement {
                                id: z as u32 + 1,
                                image: request.image.0 as u32,
                                x,
                                y,
                                width: right - x,
                                height: bottom - y,
                            });
                        }
                    }
                }
            }
        }
        const QUADRANTS: [&str; 16] = [
            " ", "▘", "▝", "▀", "▖", "▌", "▞", "▛", "▗", "▚", "▐", "▜", "▄", "▙", "▟", "█",
        ];
        const BOXES: [&str; 16] = [
            " ", "╵", "╴", "└", "╷", "│", "┌", "├", "╶", "┘", "─", "┴", "┐", "┤", "┬", "┼",
        ];
        for cell_y in damage_bounds.1..damage_bounds.3 {
            for cell_x in damage_bounds.0..damage_bounds.2 {
                if !self.damaged[cell_y * self.columns + cell_x] {
                    continue;
                }
                let pixel_x = cell_x * 2;
                let pixel_y = cell_y * 2;
                let stride = self.columns * 2;
                let pixels = [
                    self.pixels[pixel_y * stride + pixel_x],
                    self.pixels[pixel_y * stride + pixel_x + 1],
                    self.pixels[(pixel_y + 1) * stride + pixel_x],
                    self.pixels[(pixel_y + 1) * stride + pixel_x + 1],
                ];
                let index = cell_y * self.columns + cell_x;
                let glyph = self.glyphs[index].as_ref();
                let box_cell = self.boxes[index];
                let pixel_z = pixels.iter().map(|pixel| pixel.z).max().unwrap_or(0);
                let background = {
                    let colors = pixels.map(|pixel| pixel.color);
                    Color::from_rgba8(
                        (colors.iter().map(|color| u16::from(color.red)).sum::<u16>() / 4) as u8,
                        (colors
                            .iter()
                            .map(|color| u16::from(color.green))
                            .sum::<u16>()
                            / 4) as u8,
                        (colors
                            .iter()
                            .map(|color| u16::from(color.blue))
                            .sum::<u16>()
                            / 4) as u8,
                        255,
                    )
                };
                if let Some(glyph) = glyph
                    && pixel_z <= glyph.z
                    && box_cell.z <= glyph.z
                {
                    let text = match glyph.text {
                        GlyphText::Static(text) => text,
                        GlyphText::Run { text, start, end } => {
                            let run = self.text_run_index(text);
                            &self.text_runs.get_index(run).text[start..end]
                        }
                    };
                    let cell = &mut self.cells[index];
                    cell.text.clear();
                    cell.text.push_str(text);
                    cell.foreground = glyph.color;
                    cell.background = background;
                    cell.bold = glyph.bold;
                    continue;
                }
                if box_cell.edges != 0 && pixel_z <= box_cell.z {
                    let text = if box_cell.rounded {
                        match box_cell.edges {
                            3 => "╰",
                            6 => "╭",
                            9 => "╯",
                            12 => "╮",
                            _ => BOXES[box_cell.edges as usize],
                        }
                    } else {
                        BOXES[box_cell.edges as usize]
                    };
                    let cell = &mut self.cells[index];
                    cell.text.clear();
                    cell.text.push_str(text);
                    cell.foreground = box_cell.color;
                    cell.background = background;
                    cell.bold = false;
                    continue;
                }
                let mut background = pixels[0].color;
                let mut background_count = 0;
                for candidate in pixels.map(|pixel| pixel.color) {
                    let count = pixels
                        .iter()
                        .filter(|pixel| pixel.color == candidate)
                        .count();
                    if count > background_count {
                        background = candidate;
                        background_count = count;
                    }
                }
                let foreground = pixels
                    .iter()
                    .map(|pixel| pixel.color)
                    .max_by_key(|color| distance(*color, background))
                    .unwrap_or(background);
                let mut mask = 0;
                for (bit, pixel) in pixels.iter().enumerate() {
                    if distance(pixel.color, foreground) < distance(pixel.color, background) {
                        mask |= 1 << bit;
                    }
                }
                let cell = &mut self.cells[index];
                cell.text.clear();
                cell.text.push_str(QUADRANTS[mask]);
                cell.foreground = foreground;
                cell.background = background;
                cell.bold = false;
            }
        }
        let mut style = None;
        for y in 0..self.rows {
            let mut x = 0;
            while x < self.columns {
                let index = y * self.columns + x;
                if !self.damaged[index] || self.previous[index] == self.cells[index] {
                    x += 1;
                    continue;
                }
                write!(self.output, "\x1b[{};{}H", y + 1, x + 1).unwrap();
                while x < self.columns {
                    let index = y * self.columns + x;
                    let cell = &self.cells[index];
                    if !self.damaged[index] || self.previous[index] == *cell {
                        break;
                    }
                    let next_style = (cell.foreground, cell.background, cell.bold);
                    if style != Some(next_style) {
                        write!(
                            self.output,
                            "\x1b[0;{};38;2;{};{};{};48;2;{};{};{}m",
                            if cell.bold { 1 } else { 22 },
                            cell.foreground.red,
                            cell.foreground.green,
                            cell.foreground.blue,
                            cell.background.red,
                            cell.background.green,
                            cell.background.blue,
                        )
                        .unwrap();
                        style = Some(next_style);
                    }
                    self.output.push_str(&cell.text);
                    self.previous[index].clone_from(cell);
                    x += 1;
                }
            }
        }
        self.output.push_str("\x1b[0m");
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
            let image = &mut self.images[placement.image as usize - 1];
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
        self.text_layouts.trim_to_weight();
        self.text_runs.trim_to_weight();
    }

    fn create_image(&mut self, data: ImageData) -> ImageHandle {
        data.validate();
        let id = self.images.len() as u64 + 1;
        let size = data.size;
        let width = data.texture_rect.width as usize;
        let height = data.texture_rect.height as usize;
        let bytes = data.pixels.bytes();
        let mut rgba = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let offset = y * data.stride_bytes + x * data.format.bytes_per_pixel();
                match data.format {
                    blit::image::ImageFormat::Rgb8 => {
                        rgba.extend_from_slice(&bytes[offset..offset + 3]);
                        rgba.push(255);
                    }
                    blit::image::ImageFormat::Rgba8 => {
                        rgba.extend_from_slice(&bytes[offset..offset + 4]);
                    }
                    blit::image::ImageFormat::Rgba8Premultiplied => {
                        let alpha = bytes[offset + 3];
                        for channel in &bytes[offset..offset + 3] {
                            rgba.push(if alpha == 0 {
                                0
                            } else {
                                ((*channel as u16 * 255) / u16::from(alpha)).min(255) as u8
                            });
                        }
                        rgba.push(alpha);
                    }
                    blit::image::ImageFormat::Luma8 => {
                        rgba.extend_from_slice(&[bytes[offset], bytes[offset], bytes[offset], 255]);
                    }
                    blit::image::ImageFormat::Alpha8(color) => {
                        rgba.extend_from_slice(&[
                            color.red,
                            color.green,
                            color.blue,
                            ((u16::from(color.alpha) * u16::from(bytes[offset])) / 255) as u8,
                        ]);
                    }
                }
            }
        }
        self.images.push(StoredImage {
            rgba: rgba.into_boxed_slice(),
            width,
            height,
            transmitted: false,
        });
        ImageHandle::new(ImageId(id), size)
    }

    fn text_run(&mut self, text: &str, style: TextStyle) -> TextRunId {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let query = RunQuery {
            digest: hasher.finish(),
            len: text.len(),
            font: style.font,
            size: style.size.to_bits(),
            weight: style.weight,
        };
        let next = self.next_text_run;
        let (_, index) = self.text_runs.get_or_insert_by(
            &query,
            |key, run| {
                key.digest == query.digest
                    && key.len == query.len
                    && key.font == query.font
                    && key.size == query.size
                    && key.weight == query.weight
                    && run.text.as_ref() == text
            },
            || {
                (
                    RunKey {
                        digest: query.digest,
                        len: query.len,
                        font: query.font,
                        size: query.size,
                        weight: query.weight,
                    },
                    CachedRun {
                        id: TextRunId(u64::from(next) << 32),
                        text: text.into(),
                    },
                )
            },
        );
        if self.text_runs.get_index(index).id.0 as u32 == 0 {
            let slot = u32::try_from(index + 1).expect("too many terminal text runs");
            self.text_runs
                .update_index(index, |run| run.id.0 |= u64::from(slot));
            self.next_text_run = self
                .next_text_run
                .checked_add(1)
                .expect("too many terminal text runs");
        }
        self.text_runs.get_index(index).id
    }

    fn text_offset_at_position(&mut self, request: &TextRequest, position: LogicalPoint) -> usize {
        let text = &self
            .text_runs
            .get_index(self.text_run_index(request.text))
            .text;
        let target = ((position.x - request.area.x + request.offset_x) * self.scale.x)
            .round()
            .max(0.0) as usize;
        let mut width = 0;
        for (offset, grapheme) in text.grapheme_indices(true) {
            let next = width + UnicodeWidthStr::width(grapheme).max(1);
            if target < next {
                return offset;
            }
            width = next;
        }
        text.len()
    }

    fn measure_text(&mut self, request: &TextLayoutRequest) -> LogicalSize {
        let layout = self.layout_text(request);
        let layout = self.text_layouts.get_index(layout);
        LogicalSize {
            width: layout.width as f32 * self.cell_size.width,
            height: layout.lines.len() as f32 * self.cell_size.height,
        }
    }

    fn text_cursor_rect(&mut self, request: &TextRequest, byte_offset: usize) -> LogicalRect {
        let text = &self
            .text_runs
            .get_index(self.text_run_index(request.text))
            .text;
        let before = &text[..text.floor_char_boundary(byte_offset.min(text.len()))];
        let line = before.rsplit_once('\n').map_or(before, |(_, line)| line);
        LogicalRect {
            x: request.area.x + UnicodeWidthStr::width(line) as f32 * self.cell_size.width
                - request.offset_x,
            y: request.area.y + before.matches('\n').count() as f32 * self.cell_size.height,
            width: self.cell_size.width,
            height: self.cell_size.height,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Pixel {
    color: Color,
    z: usize,
}

struct StoredImage {
    rgba: Box<[u8]>,
    width: usize,
    height: usize,
    transmitted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct KittyPlacement {
    id: u32,
    image: u32,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

#[derive(Clone, Copy, Default)]
struct BoxCell {
    edges: u8,
    color: Color,
    z: usize,
    rounded: bool,
}

impl BoxCell {
    fn paint(&mut self, edges: u8, rounded: bool, color: Color, z: usize) {
        if z > self.z {
            self.edges = edges;
            self.rounded = rounded;
            self.color = color;
            self.z = z;
        } else if z == self.z {
            self.rounded = self.edges == 0 && rounded;
            self.edges |= edges;
            self.color = color;
        }
    }
}

#[derive(Clone, Copy)]
enum GlyphText {
    Static(&'static str),
    Run {
        text: TextRunId,
        start: usize,
        end: usize,
    },
}

#[derive(Clone)]
struct Glyph {
    text: GlyphText,
    color: Color,
    bold: bool,
    z: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Cell {
    text: String,
    foreground: Color,
    background: Color,
    bold: bool,
}

impl Cell {
    fn invalid() -> Self {
        Self {
            text: String::new(),
            foreground: Color::TRANSPARENT,
            background: Color::TRANSPARENT,
            bold: false,
        }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            text: " ".into(),
            foreground: Color::WHITE,
            background: Color::BLACK,
            bold: false,
        }
    }
}

struct RunScale;

impl Scale<RunKey, CachedRun> for RunScale {
    fn weight(&self, _key: &RunKey, run: &CachedRun) -> usize {
        size_of::<CachedRun>() + run.text.len()
    }
}

struct LayoutScale;

impl Scale<LayoutKey, TextLayout> for LayoutScale {
    fn weight(&self, _key: &LayoutKey, layout: &TextLayout) -> usize {
        size_of::<TextLayout>()
            + layout.lines.len() * size_of::<Line>()
            + layout.graphemes.len() * size_of::<LayoutGrapheme>()
    }
}

struct CachedRun {
    id: TextRunId,
    text: Box<str>,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct RunKey {
    digest: u64,
    len: usize,
    font: blit::text::FontId,
    size: u32,
    weight: u16,
}

#[derive(Clone, Copy, Hash)]
struct RunQuery {
    digest: u64,
    len: usize,
    font: blit::text::FontId,
    size: u32,
    weight: u16,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct LayoutKey {
    text: TextRunId,
    max_columns: Option<usize>,
    max_lines: usize,
    wrap: TextWrap,
}

#[derive(Clone, Copy, Default)]
struct Line {
    start: usize,
    end: usize,
    width: usize,
}

#[derive(Clone, Copy)]
struct LayoutGrapheme {
    start: usize,
    end: usize,
    width: usize,
}

struct TextLayout {
    lines: Box<[Line]>,
    graphemes: Box<[LayoutGrapheme]>,
    width: usize,
    truncated: bool,
}

fn blend(source: Color, destination: Color, opacity: f32) -> Color {
    let alpha = source.alpha as f32 / 255.0 * opacity.clamp(0.0, 1.0);
    let inverse = 1.0 - alpha;
    Color::from_rgba8(
        (source.red as f32 * alpha + destination.red as f32 * inverse).round() as u8,
        (source.green as f32 * alpha + destination.green as f32 * inverse).round() as u8,
        (source.blue as f32 * alpha + destination.blue as f32 * inverse).round() as u8,
        255,
    )
}

fn distance(left: Color, right: Color) -> u32 {
    u32::from(left.red.abs_diff(right.red)).pow(2)
        + u32::from(left.green.abs_diff(right.green)).pow(2)
        + u32::from(left.blue.abs_diff(right.blue)).pow(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use blit::{UiState, repaint::FullRepaint};
    use std::time::Duration;

    const PIXEL_LIKE: LogicalSize = LogicalSize {
        width: 8.0,
        height: 16.0,
    };
    const CELL_WIDTH: f32 = PIXEL_LIKE.width;
    const CELL_HEIGHT: f32 = PIXEL_LIKE.height;
    const SCALE: Scale2 = Scale2 {
        x: 1.0 / CELL_WIDTH,
        y: 1.0 / CELL_HEIGHT,
    };

    fn pixel_renderer(columns: u16, rows: u16) -> TerminalRenderer {
        TerminalRenderer::new(
            RendererConfig::new()
                .columns(columns)
                .rows(rows)
                .cell_size(PIXEL_LIKE),
        )
    }

    #[test]
    fn real_blit_text_reaches_terminal_cells() {
        let mut renderer = pixel_renderer(20, 4);
        let mut state = UiState::default();
        blit::render(
            &mut renderer,
            &mut state,
            &mut FullRepaint,
            Duration::ZERO,
            [],
            |ui| {
                use blit::widget::{Text, Widget};
                ui.clear();
                Text::new("hello terminal").render(ui);
            },
        );
        assert!(renderer.plain_text().contains("hello terminal"));
    }

    #[test]
    fn default_geometry_uses_cells() {
        let mut renderer = TerminalRenderer::new(RendererConfig::new().columns(4).rows(3));
        let mut state = UiState::default();
        let mut screen = LogicalRect::default();
        blit::render(
            &mut renderer,
            &mut state,
            &mut FullRepaint,
            Duration::ZERO,
            [],
            |ui| screen = ui.screen(),
        );
        assert_eq!(screen, renderer.screen().to_logical(Scale2::IDENTITY));
    }

    #[test]
    fn absolute_slots_use_terminal_resolution() {
        use blit::{container::Absolute, interact::WidgetId, layout::Flex};

        let mut renderer = pixel_renderer(4, 3);
        let mut state = UiState::default();
        let id = WidgetId::new("absolute resolution");
        blit::render(
            &mut renderer,
            &mut state,
            &mut FullRepaint,
            Duration::ZERO,
            [],
            |ui| {
                let mut root = ui.layout(Flex::column()).grow().open();
                root.add(|ui: &mut blit::Ui| {
                    ui.layout(Flex::column())
                        .fixed(3.0, 4.0)
                        .id(id)
                        .absolute(Absolute::at(0.0, 0.0))
                        .open();
                });
            },
        );

        let mut area = None;
        blit::render(
            &mut renderer,
            &mut state,
            &mut FullRepaint,
            Duration::ZERO,
            [],
            |ui| area = ui.geometry(id),
        );
        let area = area.unwrap();
        assert_eq!(area.width, CELL_WIDTH);
        assert_eq!(area.height, CELL_HEIGHT);
    }

    #[test]
    fn text_at_half_cell_offset_reaches_quantized_cell() {
        use blit::{command_list::ClipId, text::TextOptions};

        let mut renderer = pixel_renderer(4, 3);
        let text = renderer.text_run("x", TextStyle::default());
        let area = LogicalRect {
            x: CELL_WIDTH / 2.0,
            y: CELL_HEIGHT / 2.0,
            width: CELL_WIDTH,
            height: CELL_HEIGHT,
        };
        let mut commands = CommandList::default();
        commands.push_clear(renderer.screen());
        commands.push_text(
            TextRequest {
                text,
                area,
                offset_x: 0.0,
                color: Color::WHITE,
                style: TextStyle::default(),
                options: TextOptions::default(),
            },
            area.to_physical(SCALE),
            ClipId::default(),
        );
        renderer.render(&commands, &[renderer.screen()]);

        assert_eq!(renderer.cells[renderer.columns + 1].text, "x");
    }

    #[test]
    fn text_alignment_uses_quantized_area() {
        use blit::{command_list::ClipId, text::TextOptions};

        let mut renderer = pixel_renderer(5, 5);
        let text = renderer.text_run("x", TextStyle::default());
        let area = LogicalRect {
            x: CELL_WIDTH * 0.4,
            y: CELL_HEIGHT * 0.4,
            width: CELL_WIDTH * 3.2,
            height: CELL_HEIGHT * 3.2,
        };
        let mut commands = CommandList::default();
        commands.push_clear(renderer.screen());
        commands.push_text(
            TextRequest {
                text,
                area,
                offset_x: 0.0,
                color: Color::WHITE,
                style: TextStyle::default(),
                options: TextOptions {
                    horizontal_align: HorizontalAlign::Center,
                    vertical_align: VerticalAlign::Center,
                    ..TextOptions::default()
                },
            },
            area.to_physical(SCALE),
            ClipId::default(),
        );
        renderer.render(&commands, &[renderer.screen()]);

        assert_eq!(renderer.cells[renderer.columns + 1].text, "x");
    }

    #[test]
    fn damaged_render_matches_full_render() {
        use blit::command_list::{ClipId, Rectangle};

        let screen = pixel_renderer(8, 4).screen();
        let frame = |x: f32| {
            let mut commands = CommandList::default();
            commands.push_clear(screen);
            let background = screen.to_logical(SCALE);
            commands.push_rectangle(
                Rectangle::new(background).background(Color::from_rgba8(20, 30, 40, 255)),
                screen,
                ClipId::default(),
            );
            let accent = LogicalRect {
                x,
                y: CELL_HEIGHT,
                width: CELL_WIDTH,
                height: CELL_HEIGHT,
            };
            commands.push_rectangle(
                Rectangle::new(accent).background(Color::from_rgba8(80, 220, 180, 128)),
                accent.to_physical(SCALE),
                ClipId::default(),
            );
            commands
        };
        let old = frame(CELL_WIDTH);
        let current = frame(CELL_WIDTH * 2.0);
        let damage = [
            LogicalRect {
                x: CELL_WIDTH,
                y: CELL_HEIGHT,
                width: CELL_WIDTH,
                height: CELL_HEIGHT,
            }
            .to_physical(SCALE),
            LogicalRect {
                x: CELL_WIDTH * 2.0,
                y: CELL_HEIGHT,
                width: CELL_WIDTH,
                height: CELL_HEIGHT,
            }
            .to_physical(SCALE),
        ];

        let mut incremental = pixel_renderer(8, 4);
        incremental.render(&old, &[screen]);
        incremental.render(&current, &damage);
        let mut full = pixel_renderer(8, 4);
        full.render(&current, &[screen]);

        assert_eq!(incremental.cells, full.cells);
    }

    #[test]
    fn higher_borders_replace_lower_edges() {
        use blit::command_list::{ClipId, Rectangle};

        let mut renderer = pixel_renderer(7, 5);
        let mut commands = CommandList::default();
        commands.push_clear(renderer.screen());
        let lower = LogicalRect {
            x: 0.0,
            y: 0.0,
            width: CELL_WIDTH * 4.0,
            height: CELL_HEIGHT * 5.0,
        };
        commands.push_rectangle(
            Rectangle::new(lower).solid_border(1.0, Color::WHITE),
            lower.to_physical(SCALE),
            ClipId::default(),
        );
        let upper = LogicalRect {
            x: CELL_WIDTH * 2.0,
            y: CELL_HEIGHT,
            width: CELL_WIDTH * 3.0,
            height: CELL_HEIGHT * 3.0,
        };
        commands.push_rectangle(
            Rectangle::new(upper)
                .background(Color::BLACK)
                .solid_border(1.0, Color::WHITE),
            upper.to_physical(SCALE),
            ClipId::default(),
        );
        renderer.render(&commands, &[renderer.screen()]);

        assert_eq!(renderer.cells[renderer.columns + 3].text, "─");
    }

    #[test]
    fn word_wrap_keeps_words_intact() {
        let mut renderer = pixel_renderer(20, 4);
        let text = renderer.text_run("hello world", TextStyle::default());
        let layout = renderer.layout_text(&TextLayoutRequest {
            text,
            style: TextStyle::default(),
            wrap: TextWrap::Word,
            max_width: Some(CELL_WIDTH * 7.0),
            max_lines: None,
        });
        let layout = renderer.text_layouts.get_index(layout);
        assert_eq!(layout.lines.len(), 2);
        assert_eq!(layout.lines[0].width, 5);
        assert_eq!(layout.lines[1].width, 5);
    }

    #[test]
    fn text_runs_and_layouts_are_reused() {
        let mut renderer = pixel_renderer(20, 4);
        let style = TextStyle::default();
        let text = renderer.text_run("cached text", style);
        assert_eq!(renderer.text_run("cached text", style), text);
        let request = TextLayoutRequest {
            text,
            style,
            wrap: TextWrap::Word,
            max_width: Some(CELL_WIDTH * 8.0),
            max_lines: None,
        };
        let layout = renderer.layout_text(&request);
        assert_eq!(renderer.layout_text(&request), layout);
    }

    #[test]
    fn rounded_rectangle_background_uses_its_border_cells() {
        use blit::{
            container::{Sizing, Slot},
            geometry::Sides,
            layout::Flex,
            style::Style,
            widget::Rectangle,
        };

        let background = Color::from_rgba8(80, 120, 160, 255);
        let mut renderer = pixel_renderer(8, 4);
        let mut state = UiState::default();
        blit::render(
            &mut renderer,
            &mut state,
            &mut FullRepaint,
            Duration::ZERO,
            [],
            |ui| {
                ui.clear();
                let mut root = ui
                    .layout(Flex::column().padding(Sides::all(3.0)))
                    .grow()
                    .open();
                root.add(
                    Rectangle::new()
                        .slot(Slot::new().width(Sizing::grow()).height(Sizing::grow()))
                        .style(
                            Style::new()
                                .background(background)
                                .solid_border(1.0, Color::WHITE)
                                .uniform_radius(1.0),
                        ),
                );
            },
        );

        assert_eq!(renderer.cells[renderer.columns + 1].text, "╭");
        assert_eq!(renderer.cells[renderer.columns + 1].background, background);
    }

    #[test]
    fn border_cells_are_interactive() {
        use blit::{
            geometry::{LogicalPoint, Sides},
            input::{Input, Modifiers, PointerButton},
            interact::{Sense, WidgetId},
            layout::Flex,
            style::Style,
        };

        let mut renderer = pixel_renderer(4, 3);
        let mut state = UiState::default();
        let id = WidgetId::new("cell border interaction");
        let mut clicked = false;
        {
            let mut render = |ui: &mut blit::Ui| {
                clicked |= ui.interact(id, Sense::CLICK).clicked;
                ui.clear();
                let mut root = ui
                    .layout(Flex::column().padding(Sides::x(CELL_WIDTH / 2.0)))
                    .grow()
                    .open();
                root.add(|ui: &mut blit::Ui| {
                    ui.layout(Flex::column())
                        .id(id)
                        .grow()
                        .style(Style::new().solid_border(1.0, Color::WHITE))
                        .open();
                });
            };
            blit::render(
                &mut renderer,
                &mut state,
                &mut FullRepaint,
                Duration::ZERO,
                [],
                &mut render,
            );
            assert_eq!(renderer.cells[2].text, "┐");

            let position = LogicalPoint {
                x: CELL_WIDTH * 2.5,
                y: CELL_HEIGHT / 2.0,
            };
            blit::render(
                &mut renderer,
                &mut state,
                &mut FullRepaint,
                Duration::ZERO,
                [
                    Input::PointerDown {
                        position,
                        button: PointerButton::Primary,
                        modifiers: Modifiers::NONE,
                    },
                    Input::PointerUp {
                        position,
                        button: PointerButton::Primary,
                        modifiers: Modifiers::NONE,
                        leave: false,
                    },
                ],
                &mut render,
            );
        }

        assert!(clicked);
    }

    #[test]
    fn disjoint_interaction_clip_is_empty() {
        let renderer = pixel_renderer(4, 3);
        assert_eq!(
            renderer.interaction_area(
                LogicalRect {
                    x: 0.0,
                    y: 0.0,
                    width: CELL_WIDTH,
                    height: CELL_HEIGHT,
                },
                LogicalRect {
                    x: CELL_WIDTH * 3.0,
                    y: 0.0,
                    width: CELL_WIDTH,
                    height: CELL_HEIGHT,
                },
            ),
            None
        );
    }

    #[test]
    fn boxes_and_kitty_images_use_terminal_protocols() {
        use blit::{
            container::{Sizing, Slot},
            image::{ImageData, ImageFormat, ImagePixels},
            layout::Flex,
            style::Style,
            widget::Image,
        };

        let mut renderer = pixel_renderer(8, 4);
        let image = renderer.create_image(ImageData::new(
            ImagePixels::Static(&[255, 0, 0, 255]),
            ImageFormat::Rgba8,
            1,
            1,
        ));
        let mut state = UiState::default();
        blit::render(
            &mut renderer,
            &mut state,
            &mut FullRepaint,
            Duration::ZERO,
            [],
            |ui| {
                ui.clear();
                let mut root = ui
                    .layout(Flex::column())
                    .grow()
                    .style(Style::new().solid_border(1.0, Color::WHITE))
                    .open();
                root.add(
                    Image::new(&image).slot(
                        Slot::new()
                            .width(Sizing::fixed(CELL_WIDTH))
                            .height(Sizing::fixed(CELL_HEIGHT)),
                    ),
                );
            },
        );
        assert!(renderer.plain_text().contains('┌'));
        let output = std::str::from_utf8(renderer.output()).unwrap();
        assert!(output.contains("\x1b_Ga=t,f=32"));
        assert!(output.contains("\x1b_Ga=p"));
    }
}
