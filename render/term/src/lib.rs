use std::{
    collections::hash_map::DefaultHasher,
    fmt::Write as _,
    hash::{Hash, Hasher},
    io,
    mem::size_of,
};

pub mod color;
pub mod command_list;
pub mod image;
pub mod text;

use crate::{
    color::Color,
    command_list::{BlockTitle, Border, BorderSides, BorderStyle, Command, CommandList},
    image::{ImageData, ImageHandle, ImageId},
    text::{
        HorizontalAlign, TextAttributes, TextLayoutRequest, TextOverflow, TextRequest, TextRunId,
        TextWrap, VerticalAlign,
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use blit::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect};
use blit_cache::{DeferredCache, Scale};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const TEXT_RUN_CACHE_CAPACITY: usize = 2 * 1024 * 1024;
const TEXT_LAYOUT_CACHE_CAPACITY: usize = 4 * 1024 * 1024;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct RendererConfig {
        new(),
        columns: u16 = 80,
        rows: u16 = 24,
    }
}

pub struct TerminalRenderer {
    columns: usize,
    rows: usize,
    text_runs: DeferredCache<RunKey, CachedRun, RunScale>,
    text_layouts: DeferredCache<LayoutKey, TextLayout, LayoutScale>,
    next_text_run: u32,
    layout_lines: Vec<Line>,
    layout_graphemes: Vec<LayoutGrapheme>,
    images: Vec<StoredImage>,
    next_image: u32,
    kitty_placements: Vec<KittyPlacement>,
    presented_kitty_placements: Vec<KittyPlacement>,
    backgrounds: Vec<Background>,
    glyphs: Vec<Option<Glyph>>,
    boxes: Vec<BoxCell>,
    cells: Vec<Cell>,
    previous: Vec<Cell>,
    damaged: Vec<bool>,
    output: String,
}

impl TerminalRenderer {
    pub fn new(config: RendererConfig) -> Self {
        let RendererConfig { columns, rows } = config;
        let columns = usize::from(columns.max(1));
        let rows = usize::from(rows.max(1));
        Self {
            columns,
            rows,
            text_runs: DeferredCache::new(RunScale, TEXT_RUN_CACHE_CAPACITY),
            text_layouts: DeferredCache::new(LayoutScale, TEXT_LAYOUT_CACHE_CAPACITY),
            next_text_run: 1,
            layout_lines: Vec::new(),
            layout_graphemes: Vec::new(),
            images: Vec::new(),
            next_image: 1,
            kitty_placements: Vec::new(),
            presented_kitty_placements: Vec::new(),
            backgrounds: vec![Background::default(); columns * rows],
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
        let RendererConfig { columns, rows } = config;
        let columns = usize::from(columns.max(1));
        let rows = usize::from(rows.max(1));
        if self.columns == columns && self.rows == rows {
            return;
        }
        self.columns = columns;
        self.rows = rows;
        self.backgrounds
            .resize(columns * rows, Background::default());
        self.glyphs.resize(columns * rows, None);
        self.boxes.resize(columns * rows, BoxCell::default());
        self.cells.resize(columns * rows, Cell::default());
        self.previous = vec![Cell::invalid(); columns * rows];
        self.damaged = vec![true; columns * rows];
    }

    fn cell_bounds(&self, area: LogicalRect) -> (usize, usize, usize, usize) {
        (
            area.x.round().clamp(0.0, self.columns as f32) as usize,
            area.y.round().clamp(0.0, self.rows as f32) as usize,
            (area.x + area.width)
                .round()
                .clamp(0.0, self.columns as f32) as usize,
            (area.y + area.height).round().clamp(0.0, self.rows as f32) as usize,
        )
    }

    fn clear_damaged(&mut self, bounds: (usize, usize, usize, usize)) {
        let (left, top, right, bottom) = bounds;
        for y in top..bottom {
            for x in left..right {
                let index = y * self.columns + x;
                if !self.damaged[index] {
                    continue;
                }
                self.backgrounds[index] = Background::default();
                self.glyphs[index] = None;
                self.boxes[index] = BoxCell::default();
            }
        }
    }

    pub fn output(&self) -> &[u8] {
        self.output.as_bytes()
    }

    pub fn clear_kitty_graphics(&mut self, output: &mut impl io::Write) -> io::Result<()> {
        for image in &mut self.images {
            if image.transmitted {
                write!(output, "\x1b_Ga=d,d=I,i={},q=2\x1b\\", image.handle.id().0)?;
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
            .map(|width| width.floor().max(0.0) as usize);
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

impl TerminalRenderer {
    pub fn interaction_area(&self, area: LogicalRect, clip: LogicalRect) -> Option<LogicalRect> {
        let (mut left, mut top, mut right, mut bottom) = self.cell_bounds(area);
        left = left.max((clip.x - 0.5).ceil().clamp(0.0, self.columns as f32) as usize);
        top = top.max((clip.y - 0.5).ceil().clamp(0.0, self.rows as f32) as usize);
        right = right.min(
            (clip.x + clip.width - 0.5)
                .ceil()
                .clamp(0.0, self.columns as f32) as usize,
        );
        bottom = bottom.min(
            (clip.y + clip.height - 0.5)
                .ceil()
                .clamp(0.0, self.rows as f32) as usize,
        );
        (right > left && bottom > top).then(|| LogicalRect {
            x: left as f32,
            y: top as f32,
            width: (right - left) as f32,
            height: (bottom - top) as f32,
        })
    }

    fn paint_border_cell(
        &mut self,
        x: usize,
        y: usize,
        edges: u8,
        border: Border,
        z: usize,
        clip: LogicalRect,
    ) {
        if edges == 0 || !self.damaged[y * self.columns + x] {
            return;
        }
        if clip.contains(LogicalPoint::new(x as f32 + 0.5, y as f32 + 0.5)) {
            self.boxes[y * self.columns + x].paint(edges, border.style, border.color, z);
        }
    }

    fn block_title_width(&mut self, title: BlockTitle) -> usize {
        let layout = self.layout_text(&TextLayoutRequest::new(title.text));
        self.text_layouts.get_index(layout).lines[0].width
    }

    fn paint_block_title(
        &mut self,
        title: BlockTitle,
        x: usize,
        y: usize,
        width: usize,
        z: usize,
        clip: LogicalRect,
    ) {
        if width == 0 {
            return;
        }
        let (clip_left, clip_top, clip_right, clip_bottom) = self.cell_bounds(clip);
        let layout = self.layout_text(&TextLayoutRequest::new(title.text));
        let layout = self.text_layouts.get_index(layout);
        let line = layout.lines[0];
        let mut column = 0;
        for grapheme in &layout.graphemes[line.start..line.end] {
            if column + grapheme.width > width {
                break;
            }
            let x = x + column;
            if x >= clip_left
                && x + grapheme.width <= clip_right
                && y >= clip_top
                && y < clip_bottom
            {
                for continuation in 0..grapheme.width {
                    let index = y * self.columns + x + continuation;
                    if self.damaged[index] {
                        self.glyphs[index] = Some(Glyph {
                            text: if continuation == 0 {
                                GlyphText::Run {
                                    text: title.text,
                                    start: grapheme.start,
                                    end: grapheme.end,
                                }
                            } else {
                                GlyphText::Static("")
                            },
                            color: title.color,
                            attributes: title.attributes,
                            z,
                        });
                    }
                }
            }
            column += grapheme.width;
        }
    }

    fn paint_block_title_row(
        &mut self,
        titles: [Option<BlockTitle>; 3],
        y: usize,
        left: usize,
        right: usize,
        z: usize,
        clip: LogicalRect,
    ) {
        let available = right.saturating_sub(left);
        if available == 0 {
            return;
        }
        let left_width = titles[0].map_or(0, |title| self.block_title_width(title));
        let right_width = titles[2].map_or(0, |title| self.block_title_width(title));
        let left_width = left_width.min(available.saturating_sub(right_width.min(available / 2)));
        let right_width = right_width.min(available - left_width);
        let right_start = right - right_width;
        if let Some(title) = titles[0] {
            self.paint_block_title(title, left, y, left_width, z, clip);
        }
        if let Some(title) = titles[2] {
            self.paint_block_title(title, right_start, y, right_width, z, clip);
        }
        if let Some(title) = titles[1] {
            let width = self
                .block_title_width(title)
                .min(right_start.saturating_sub(left + left_width));
            let centered = left + (available - width) / 2;
            let start = centered.clamp(left + left_width, right_start - width);
            self.paint_block_title(title, start, y, width, z, clip);
        }
    }

    pub fn render(&mut self, commands: &CommandList, damage: &[PhysicalRect]) {
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
        let logical_screen = LogicalRect::new(
            screen.x as f32,
            screen.y as f32,
            screen.width as f32,
            screen.height as f32,
        );
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
                Command::Block(block) => {
                    let (left, top, right, bottom) = self.cell_bounds(block.area);
                    if let Some(background) = block.background {
                        for y in top.max(damage_bounds.1)..bottom.min(damage_bounds.3) {
                            for x in left.max(damage_bounds.0)..right.min(damage_bounds.2) {
                                let index = y * self.columns + x;
                                if !self.damaged[index]
                                    || !clip
                                        .contains(LogicalPoint::new(x as f32 + 0.5, y as f32 + 0.5))
                                {
                                    continue;
                                }
                                self.backgrounds[index] = Background {
                                    color: background,
                                    z,
                                };
                            }
                        }
                    }
                    let mut title_left = left;
                    let mut title_right = right;
                    if let Some(border) = block.border
                        && right > left
                        && bottom > top
                    {
                        let right = right - 1;
                        let bottom = bottom - 1;
                        if border.sides.contains(BorderSides::TOP) {
                            for x in left..=right {
                                let edges = u8::from(x != left) * 8 | u8::from(x != right) * 2;
                                self.paint_border_cell(x, top, edges, border, z, clip);
                            }
                        }
                        if border.sides.contains(BorderSides::BOTTOM) {
                            for x in left..=right {
                                let edges = u8::from(x != left) * 8 | u8::from(x != right) * 2;
                                self.paint_border_cell(x, bottom, edges, border, z, clip);
                            }
                        }
                        if border.sides.contains(BorderSides::LEFT) {
                            for y in top..=bottom {
                                let edges = u8::from(y != top) | u8::from(y != bottom) * 4;
                                self.paint_border_cell(left, y, edges, border, z, clip);
                            }
                            title_left = title_left.saturating_add(1);
                        }
                        if border.sides.contains(BorderSides::RIGHT) {
                            for y in top..=bottom {
                                let edges = u8::from(y != top) | u8::from(y != bottom) * 4;
                                self.paint_border_cell(right, y, edges, border, z, clip);
                            }
                            title_right = title_right.saturating_sub(1);
                        }
                    }
                    if top < bottom {
                        self.paint_block_title_row(
                            [block.titles[0], block.titles[1], block.titles[2]],
                            top,
                            title_left,
                            title_right,
                            z,
                            clip,
                        );
                        self.paint_block_title_row(
                            [block.titles[3], block.titles[4], block.titles[5]],
                            bottom - 1,
                            title_left,
                            title_right,
                            z,
                            clip,
                        );
                    }
                }
                Command::Shadow(shadow) => {
                    let area = self.cell_bounds(shadow.area);
                    let shifted = LogicalRect {
                        x: shadow.area.x + shadow.offset_x,
                        y: shadow.area.y + shadow.offset_y,
                        ..shadow.area
                    };
                    let (left, top, right, bottom) = self.cell_bounds(shifted);
                    for y in top.max(damage_bounds.1)..bottom.min(damage_bounds.3) {
                        for x in left.max(damage_bounds.0)..right.min(damage_bounds.2) {
                            if (area.0..area.2).contains(&x) && (area.1..area.3).contains(&y) {
                                continue;
                            }
                            let index = y * self.columns + x;
                            if !self.damaged[index]
                                || !clip.contains(LogicalPoint::new(x as f32 + 0.5, y as f32 + 0.5))
                            {
                                continue;
                            }
                            self.backgrounds[index] = Background {
                                color: shadow.color,
                                z,
                            };
                        }
                    }
                }
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
                        wrap: request.options.wrap,
                        max_width: Some(request.area.width),
                        max_lines: request.options.max_lines,
                    };
                    let layout = self.layout_text(&layout_request);
                    let layout = self.text_layouts.get_index(layout);
                    let ellipsis = request.options.overflow == TextOverflow::Ellipsis
                        && (layout.truncated || layout.width as f32 > request.area.width);
                    let maximum = request.area.width.floor().max(1.0) as usize;
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
                        let line_ellipsis = ellipsis && line_index + 1 == layout.lines.len();
                        if line_ellipsis {
                            while line_width >= maximum && line_end != line.start {
                                line_end -= 1;
                                line_width -= layout.graphemes[line_end].width;
                            }
                            line_width += 1;
                        }
                        let start_x = match request.options.horizontal_align {
                            HorizontalAlign::Left => {
                                area_left as isize - request.offset_x.round() as isize
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
                                        attributes: request.attributes,
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
                                            attributes: request.attributes,
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
                        let x = area.x.floor().max(0.0) as usize;
                        let y = area.y.floor().max(0.0) as usize;
                        let right = (area.x + area.width).ceil().min(self.columns as f32) as usize;
                        let bottom = (area.y + area.height).ceil().min(self.rows as f32) as usize;
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
        const SINGLE_BOXES: [&str; 16] = [
            " ", "╵", "╴", "└", "╷", "│", "┌", "├", "╶", "┘", "─", "┴", "┐", "┤", "┬", "┼",
        ];
        const DOUBLE_BOXES: [&str; 16] = [
            " ", "╵", "╴", "╚", "╷", "║", "╔", "╠", "╶", "╝", "═", "╩", "╗", "╣", "╦", "╬",
        ];
        const HEAVY_BOXES: [&str; 16] = [
            " ", "╹", "╺", "┗", "╻", "┃", "┏", "┣", "╸", "┛", "━", "┻", "┓", "┫", "┳", "╋",
        ];
        for cell_y in damage_bounds.1..damage_bounds.3 {
            for cell_x in damage_bounds.0..damage_bounds.2 {
                let index = cell_y * self.columns + cell_x;
                if !self.damaged[index] {
                    continue;
                }
                let background = self.backgrounds[index];
                let glyph = self.glyphs[index].as_ref();
                let box_cell = self.boxes[index];
                if let Some(glyph) = glyph
                    && background.z <= glyph.z
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
                    cell.background = background.color;
                    cell.attributes = glyph.attributes;
                    continue;
                }
                if box_cell.edges != 0 && background.z <= box_cell.z {
                    let text = match box_cell.style {
                        BorderStyle::Rounded => match box_cell.edges {
                            3 => "╰",
                            6 => "╭",
                            9 => "╯",
                            12 => "╮",
                            _ => SINGLE_BOXES[box_cell.edges as usize],
                        },
                        BorderStyle::Single => SINGLE_BOXES[box_cell.edges as usize],
                        BorderStyle::Double => DOUBLE_BOXES[box_cell.edges as usize],
                        BorderStyle::Heavy => HEAVY_BOXES[box_cell.edges as usize],
                    };
                    let cell = &mut self.cells[index];
                    cell.text.clear();
                    cell.text.push_str(text);
                    cell.foreground = box_cell.color;
                    cell.background = background.color;
                    cell.attributes = TextAttributes::NONE;
                    continue;
                }
                let cell = &mut self.cells[index];
                cell.text.clear();
                cell.text.push(' ');
                cell.foreground = Color::Reset;
                cell.background = background.color;
                cell.attributes = TextAttributes::NONE;
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
                    let next_style = (cell.foreground, cell.background, cell.attributes);
                    if style != Some(next_style) {
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
                        write_color(&mut self.output, cell.foreground, true);
                        write_color(&mut self.output, cell.background, false);
                        self.output.push('m');
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
        self.text_runs.trim_to_weight();
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        data.validate();
        let id = self.next_image;
        self.next_image = self
            .next_image
            .checked_add(1)
            .expect("too many terminal images");
        let size = data.size;
        let width = data.texture_rect.width as usize;
        let height = data.texture_rect.height as usize;
        let texture_x = data.texture_rect.x as usize;
        let texture_y = data.texture_rect.y as usize;
        let bytes = data.pixels.bytes();
        let mut rgba = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            for x in 0..width {
                let offset = (texture_y + y) * data.stride_bytes
                    + (texture_x + x) * data.format.bytes_per_pixel();
                match data.format {
                    crate::image::ImageFormat::Rgb8 => {
                        rgba.extend_from_slice(&bytes[offset..offset + 3]);
                        rgba.push(255);
                    }
                    crate::image::ImageFormat::Rgba8 => {
                        rgba.extend_from_slice(&bytes[offset..offset + 4]);
                    }
                    crate::image::ImageFormat::Rgba8Premultiplied => {
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
                    crate::image::ImageFormat::Luma8 => {
                        rgba.extend_from_slice(&[bytes[offset], bytes[offset], bytes[offset], 255]);
                    }
                    crate::image::ImageFormat::Alpha8(color) => {
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
        let handle = ImageHandle::new(ImageId(u64::from(id)), size);
        self.images.push(StoredImage {
            handle: handle.clone(),
            rgba: rgba.into_boxed_slice(),
            width,
            height,
            transmitted: false,
        });
        handle
    }

    pub fn text_run(&mut self, text: &str) -> TextRunId {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let query = RunKey {
            digest: hasher.finish(),
            len: text.len(),
        };
        let next = self.next_text_run;
        let (_, index) = self.text_runs.get_or_insert_by(
            &query,
            |key, run| *key == query && run.text.as_ref() == text,
            || {
                (
                    query,
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

    pub fn text_offset_at_position(
        &mut self,
        request: &TextRequest,
        position: LogicalPoint,
    ) -> usize {
        let text = &self
            .text_runs
            .get_index(self.text_run_index(request.text))
            .text;
        let target = (position.x - request.area.x + request.offset_x)
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

    pub fn measure_text(&mut self, request: &TextLayoutRequest) -> LogicalSize {
        let layout = self.layout_text(request);
        let layout = self.text_layouts.get_index(layout);
        LogicalSize {
            width: layout.width as f32,
            height: layout.lines.len() as f32,
        }
    }

    pub fn text_cursor_rect(&mut self, request: &TextRequest, byte_offset: usize) -> LogicalRect {
        let text = &self
            .text_runs
            .get_index(self.text_run_index(request.text))
            .text;
        let before = &text[..text.floor_char_boundary(byte_offset.min(text.len()))];
        let line = before.rsplit_once('\n').map_or(before, |(_, line)| line);
        LogicalRect {
            x: request.area.x + UnicodeWidthStr::width(line) as f32 - request.offset_x,
            y: request.area.y + before.matches('\n').count() as f32,
            width: 1.0,
            height: 1.0,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Background {
    color: Color,
    z: usize,
}

struct StoredImage {
    handle: ImageHandle,
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
    style: BorderStyle,
}

impl BoxCell {
    fn paint(&mut self, edges: u8, style: BorderStyle, color: Color, z: usize) {
        if z > self.z {
            self.edges = edges;
            self.style = style;
            self.color = color;
            self.z = z;
        } else if z == self.z {
            if self.edges == 0 {
                self.style = style;
            }
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
    attributes: TextAttributes,
    z: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Cell {
    text: String,
    foreground: Color,
    background: Color,
    attributes: TextAttributes,
    valid: bool,
}

impl Cell {
    fn invalid() -> Self {
        Self {
            valid: false,
            ..Self::default()
        }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            text: " ".into(),
            foreground: Color::Reset,
            background: Color::Reset,
            attributes: TextAttributes::NONE,
            valid: true,
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

fn write_color(output: &mut String, color: Color, foreground: bool) {
    match color {
        Color::Reset => output.push_str(if foreground { ";39" } else { ";49" }),
        Color::Indexed(index @ 0..=7) => {
            let base = if foreground { 30 } else { 40 };
            write!(output, ";{}", base + index).unwrap();
        }
        Color::Indexed(index @ 8..=15) => {
            let base = if foreground { 90 } else { 100 };
            write!(output, ";{}", base + index - 8).unwrap();
        }
        Color::Indexed(index) => {
            let prefix = if foreground { 38 } else { 48 };
            write!(output, ";{prefix};5;{index}").unwrap();
        }
        Color::Rgb(red, green, blue) => {
            let prefix = if foreground { 38 } else { 48 };
            write!(output, ";{prefix};2;{red};{green};{blue}").unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use blit::Scale2;

    const CELL_WIDTH: f32 = 1.0;
    const CELL_HEIGHT: f32 = 1.0;
    const SCALE: Scale2 = Scale2::IDENTITY;

    fn renderer(columns: u16, rows: u16) -> TerminalRenderer {
        TerminalRenderer::new(RendererConfig::new().columns(columns).rows(rows))
    }

    #[test]
    fn text_at_half_cell_offset_reaches_quantized_cell() {
        use crate::command_list::ClipId;

        let mut renderer = renderer(4, 3);
        let text = renderer.text_run("x");
        let area = LogicalRect {
            x: CELL_WIDTH / 2.0,
            y: CELL_HEIGHT / 2.0,
            width: CELL_WIDTH,
            height: CELL_HEIGHT,
        };
        let mut commands = CommandList::default();
        commands.push_clear(renderer.screen());
        commands.push_text(
            TextRequest::new(text, area)
                .attributes(TextAttributes::BOLD | TextAttributes::STRIKETHROUGH),
            area.to_physical(SCALE),
            ClipId::default(),
        );
        renderer.render(&commands, &[renderer.screen()]);

        assert_eq!(renderer.cells[renderer.columns + 1].text, "x");
        assert_eq!(
            renderer.cells[renderer.columns + 1].attributes,
            TextAttributes::BOLD | TextAttributes::STRIKETHROUGH
        );
    }

    #[test]
    fn text_alignment_uses_quantized_area() {
        use crate::{
            command_list::ClipId,
            text::{HorizontalAlign, TextOptions, VerticalAlign},
        };

        let mut renderer = renderer(5, 5);
        let text = renderer.text_run("x");
        let area = LogicalRect {
            x: CELL_WIDTH * 0.4,
            y: CELL_HEIGHT * 0.4,
            width: CELL_WIDTH * 3.2,
            height: CELL_HEIGHT * 3.2,
        };
        let mut commands = CommandList::default();
        commands.push_clear(renderer.screen());
        commands.push_text(
            TextRequest::new(text, area).options(
                TextOptions::new()
                    .horizontal_align(HorizontalAlign::Center)
                    .vertical_align(VerticalAlign::Center),
            ),
            area.to_physical(SCALE),
            ClipId::default(),
        );
        renderer.render(&commands, &[renderer.screen()]);

        assert_eq!(renderer.cells[renderer.columns + 1].text, "x");
    }

    #[test]
    fn damaged_render_matches_full_render() {
        use crate::command_list::{Block, ClipId};

        let screen = renderer(8, 4).screen();
        let frame = |x: f32| {
            let mut commands = CommandList::default();
            commands.push_clear(screen);
            let background = screen.to_logical(SCALE);
            commands.push_block(
                Block::new(background).background(Color::Rgb(20, 30, 40)),
                screen,
                ClipId::default(),
            );
            let accent = LogicalRect {
                x,
                y: CELL_HEIGHT,
                width: CELL_WIDTH,
                height: CELL_HEIGHT,
            };
            commands.push_block(
                Block::new(accent).background(Color::Rgb(80, 220, 180)),
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

        let mut incremental = renderer(8, 4);
        incremental.render(&old, &[screen]);
        incremental.render(&current, &damage);
        let mut full = renderer(8, 4);
        full.render(&current, &[screen]);

        assert_eq!(incremental.cells, full.cells);
    }

    #[test]
    fn higher_borders_replace_lower_edges() {
        use crate::command_list::{Block, Border, ClipId};

        let mut renderer = renderer(7, 5);
        let mut commands = CommandList::default();
        commands.push_clear(renderer.screen());
        let lower = LogicalRect {
            x: 0.0,
            y: 0.0,
            width: CELL_WIDTH * 4.0,
            height: CELL_HEIGHT * 5.0,
        };
        commands.push_block(
            Block::new(lower).border(Border::new(Color::WHITE)),
            lower.to_physical(SCALE),
            ClipId::default(),
        );
        let upper = LogicalRect {
            x: CELL_WIDTH * 2.0,
            y: CELL_HEIGHT,
            width: CELL_WIDTH * 3.0,
            height: CELL_HEIGHT * 3.0,
        };
        commands.push_block(
            Block::new(upper)
                .background(Color::BLACK)
                .border(Border::new(Color::WHITE).style(BorderStyle::Rounded)),
            upper.to_physical(SCALE),
            ClipId::default(),
        );
        renderer.render(&commands, &[renderer.screen()]);

        assert_eq!(renderer.cells[renderer.columns + 2].text, "╭");
        assert_eq!(renderer.cells[renderer.columns + 3].text, "─");
    }

    #[test]
    fn shadow_fills_exposed_cells() {
        use crate::command_list::{BoxShadow, ClipId};

        let mut renderer = renderer(6, 4);
        let area = LogicalRect::new(1.0, 0.0, 3.0, 2.0);
        let shifted = LogicalRect::new(2.0, 1.0, 3.0, 2.0);
        let mut commands = CommandList::default();
        commands.push_clear(renderer.screen());
        commands.push_shadow(
            BoxShadow::new(area, Color::WHITE),
            shifted.to_physical(SCALE),
            ClipId::default(),
        );
        renderer.render(&commands, &[renderer.screen()]);

        for (x, y) in [(4, 1), (2, 2), (3, 2), (4, 2)] {
            assert_eq!(
                renderer.cells[y * renderer.columns + x].background,
                Color::WHITE
            );
        }
    }

    #[test]
    fn block_renders_aligned_titles_and_border_styles() {
        use crate::command_list::{Block, BlockTitle, Border, ClipId, TitlePosition};

        let mut renderer = renderer(20, 4);
        let replaced = renderer.text_run("X");
        let top_left = renderer.text_run("L");
        let top_center = renderer.text_run("C");
        let top_right = renderer.text_run("R");
        let bottom_left = renderer.text_run("l");
        let bottom_center = renderer.text_run("c");
        let bottom_right = renderer.text_run("r");
        let area = renderer.screen().to_logical(SCALE);
        let mut commands = CommandList::default();
        commands.push_clear(renderer.screen());
        commands.push_block(
            Block::new(area)
                .border(Border::new(Color::WHITE).style(BorderStyle::Double))
                .title(BlockTitle::new(replaced))
                .title(BlockTitle::new(top_left))
                .title(BlockTitle::new(top_center).position(TitlePosition::TopCenter))
                .title(BlockTitle::new(top_right).position(TitlePosition::TopRight))
                .title(BlockTitle::new(bottom_left).position(TitlePosition::BottomLeft))
                .title(BlockTitle::new(bottom_center).position(TitlePosition::BottomCenter))
                .title(BlockTitle::new(bottom_right).position(TitlePosition::BottomRight)),
            renderer.screen(),
            ClipId::default(),
        );
        renderer.render(&commands, &[renderer.screen()]);

        assert_eq!(
            renderer.plain_text(),
            "╔L═══════C════════R╗\n║                  ║\n║                  ║\n╚l═══════c════════r╝\n"
        );
    }

    #[test]
    fn word_wrap_keeps_words_intact() {
        let mut renderer = renderer(20, 4);
        let text = renderer.text_run("hello world");
        let layout = renderer.layout_text(
            &TextLayoutRequest::new(text)
                .wrap(TextWrap::Word)
                .max_width(CELL_WIDTH * 7.0),
        );
        let layout = renderer.text_layouts.get_index(layout);
        assert_eq!(layout.lines.len(), 2);
        assert_eq!(layout.lines[0].width, 5);
        assert_eq!(layout.lines[1].width, 5);
    }

    #[test]
    fn text_runs_and_layouts_are_reused() {
        let mut renderer = renderer(20, 4);
        let text = renderer.text_run("cached text");
        assert_eq!(renderer.text_run("cached text"), text);
        let request = TextLayoutRequest::new(text)
            .wrap(TextWrap::Word)
            .max_width(CELL_WIDTH * 8.0);
        let layout = renderer.layout_text(&request);
        assert_eq!(renderer.layout_text(&request), layout);
    }

    #[test]
    fn disjoint_interaction_clip_is_empty() {
        let renderer = renderer(4, 3);
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
}
