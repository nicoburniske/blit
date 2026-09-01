use std::{
    collections::hash_map::DefaultHasher,
    fmt::Write as _,
    hash::{Hash, Hasher},
    io,
    mem::size_of,
};

pub mod cell;
pub mod color;
pub mod image;
mod present;
pub mod text;

use crate::{
    color::Color,
    image::{ImageData, ImageHandle, ImageId},
    text::{Span, TextAttributes, TextLayoutRequest, TextRequest, TextRunId, TextWrap},
};
use base64::engine::general_purpose::STANDARD as BASE64;
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

pub struct TuiRenderer {
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
    frame_cells: Vec<Cell>,
    cells: Vec<Cell>,
    changed: Vec<bool>,
    output: String,
    next_placement: u32,
}

impl TuiRenderer {
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
            frame_cells: vec![Cell::default(); columns * rows],
            cells: vec![Cell::invalid(); columns * rows],
            changed: vec![true; columns * rows],
            output: String::new(),
            next_placement: 1,
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
        for cell in &self.cells {
            if let CellText::Run { run, .. } = cell.text {
                self.text_runs
                    .update_index(run as usize, |run| run.screen_references -= 1);
            }
        }
        self.columns = columns;
        self.rows = rows;
        self.frame_cells.resize(columns * rows, Cell::default());
        self.cells = vec![Cell::invalid(); columns * rows];
        self.changed = vec![true; columns * rows];
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
                Self::push_cell_text(&self.text_runs, cell.text, &mut output);
            }
            while output.ends_with(' ') {
                output.pop();
            }
            output.push('\n');
        }
        output
    }

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
        self.rich_text(&[Span::new(text)])
    }

    pub fn rich_text(&mut self, spans: &[Span<'_>]) -> TextRunId {
        let mut hasher = DefaultHasher::new();
        spans.hash(&mut hasher);
        let query = RunKey {
            digest: hasher.finish(),
            len: spans.iter().map(|span| span.text.len()).sum(),
            spans: spans.len(),
        };
        assert!(u32::try_from(query.len).is_ok(), "tui text run too long");
        let next = self.next_text_run;
        let (_, index) = self.text_runs.get_or_insert_by(
            &query,
            |key, run| {
                *key == query
                    && run.spans.iter().zip(spans).all(|(resolved, span)| {
                        run.text[resolved.start..resolved.end] == *span.text
                            && resolved.color == span.color
                            && resolved.attributes == span.attributes
                    })
            },
            || {
                let mut text = String::with_capacity(query.len);
                let mut resolved = Vec::with_capacity(spans.len());
                for span in spans {
                    let start = text.len();
                    text.push_str(span.text);
                    resolved.push(ResolvedSpan {
                        start,
                        end: text.len(),
                        color: span.color,
                        attributes: span.attributes,
                    });
                }
                (
                    query,
                    CachedRun {
                        id: TextRunId(u64::from(next) << 32),
                        text: text.into_boxed_str(),
                        screen_references: 0,
                        spans: resolved.into_boxed_slice(),
                    },
                )
            },
        );
        if self.text_runs.get_index(index).id.0 as u32 == 0 {
            let slot = u32::try_from(index + 1).expect("too many tui text runs");
            self.text_runs
                .update_index(index, |run| run.id.0 |= u64::from(slot));
            self.next_text_run = self
                .next_text_run
                .checked_add(1)
                .expect("too many tui text runs");
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CellText {
    Continuation,
    Scalar(char),
    // screen references pin cache slots while cells use them
    Run {
        run: u32,
        start: u32,
        end: u32,
        width: u16,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cell {
    text: CellText,
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
            text: CellText::Scalar(' '),
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
        size_of::<CachedRun>() + run.text.len() + run.spans.len() * size_of::<ResolvedSpan>()
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
    screen_references: usize,
    spans: Box<[ResolvedSpan]>,
}

#[derive(Clone, Copy)]
struct ResolvedSpan {
    start: usize,
    end: usize,
    color: Option<Color>,
    attributes: TextAttributes,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct RunKey {
    digest: u64,
    len: usize,
    spans: usize,
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

impl TuiRenderer {
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

    fn text_run_index(&self, id: TextRunId) -> usize {
        let index = (id.0 as u32).checked_sub(1).expect("invalid tui text run") as usize;
        assert_eq!(self.text_runs.get_index(index).id, id, "expired text run");
        index
    }

    fn set_cell(&mut self, index: usize, cell: Cell) {
        let old = match self.cells[index].text {
            CellText::Run { run, .. } => Some(run),
            _ => None,
        };
        let new = match cell.text {
            CellText::Run { run, .. } => Some(run),
            _ => None,
        };
        if old != new {
            if let Some(run) = new {
                self.text_runs
                    .update_index(run as usize, |run| run.screen_references += 1);
            }
            if let Some(run) = old {
                self.text_runs.update_index(run as usize, |run| {
                    run.screen_references -= 1;
                });
            }
        }
        self.cells[index] = cell;
    }

    fn push_cell_text(
        text_runs: &DeferredCache<RunKey, CachedRun, RunScale>,
        text: CellText,
        output: &mut String,
    ) {
        match text {
            CellText::Continuation => {}
            CellText::Scalar(character) => output.push(character),
            CellText::Run {
                run, start, end, ..
            } => {
                let run = text_runs.get_index(run as usize);
                output.push_str(&run.text[start as usize..end as usize]);
            }
        }
    }

    fn layout_text(&mut self, request: &TextLayoutRequest) -> usize {
        fn grapheme_width(grapheme: &str) -> Option<usize> {
            if grapheme.contains(char::is_control) {
                return None;
            }
            let width = UnicodeWidthStr::width(grapheme);
            (width != 0).then_some(width)
        }

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
                        let token_width = token
                            .graphemes(true)
                            .filter_map(grapheme_width)
                            .sum::<usize>();
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
                            let Some(width) = grapheme_width(grapheme) else {
                                continue;
                            };
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
                        let Some(width) = grapheme_width(grapheme) else {
                            continue;
                        };
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

fn write_color(output: &mut String, color: Color, foreground: bool) {
    match color {
        Color::Reset => output.push_str(if foreground { "39" } else { "49" }),
        Color::Indexed(index @ 0..=7) => {
            let base = if foreground { 30 } else { 40 };
            write!(output, "{}", base + index).unwrap();
        }
        Color::Indexed(index @ 8..=15) => {
            let base = if foreground { 90 } else { 100 };
            write!(output, "{}", base + index - 8).unwrap();
        }
        Color::Indexed(index) => {
            let prefix = if foreground { 38 } else { 48 };
            write!(output, "{prefix};5;{index}").unwrap();
        }
        Color::Rgb(red, green, blue) => {
            let prefix = if foreground { 38 } else { 48 };
            write!(output, "{prefix};2;{red};{green};{blue}").unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use blit::Scale2;

    use crate::{
        cell::{Cell as SurfaceCell, CellStyle},
        text::{HorizontalAlign, TextOptions, VerticalAlign},
    };

    const SCALE: Scale2 = Scale2::IDENTITY;

    fn renderer(columns: u16, rows: u16) -> TuiRenderer {
        TuiRenderer::new(RendererConfig::new().columns(columns).rows(rows))
    }

    #[test]
    fn direct_cells_are_frame_local_and_diffed() {
        let mut renderer = renderer(5, 2);
        let area = renderer.screen().to_logical(SCALE);
        renderer.begin_frame();
        renderer
            .cells(area, area)
            .write(0, 0, "hello", CellStyle::new().foreground(Color::GREEN));
        renderer.end_frame();
        assert_eq!(renderer.plain_text(), "hello\n\n");
        assert!(!renderer.output().is_empty());

        renderer.begin_frame();
        renderer
            .cells(area, area)
            .write(0, 0, "hello", CellStyle::new().foreground(Color::GREEN));
        renderer.end_frame();
        assert!(renderer.output().is_empty());

        renderer.begin_frame();
        renderer
            .cells(area, area)
            .set_cell(1, 1, SurfaceCell::new('x'));
        renderer.end_frame();
        assert_eq!(renderer.plain_text(), "\n x\n");
    }

    #[test]
    fn direct_cells_quantize_half_cell_areas() {
        let mut renderer = renderer(4, 3);
        let screen = renderer.screen().to_logical(SCALE);
        renderer.begin_frame();
        renderer
            .cells(LogicalRect::new(1.0, 0.5, 2.0, 1.0), screen)
            .clear(SurfaceCell::default().style(CellStyle::new().background(Color::CYAN)));
        renderer.end_frame();

        assert_eq!(renderer.cells[5].background, Color::CYAN);
        assert_eq!(renderer.cells[6].background, Color::CYAN);
    }

    #[test]
    fn direct_cells_support_graphemes_and_wide_overwrites() {
        let mut renderer = renderer(8, 1);
        let area = renderer.screen().to_logical(SCALE);
        let text = "e\u{301}界👍🏽";
        let family = "👨‍👩‍👧";

        renderer.begin_frame();
        {
            let mut cells = renderer.cells(area, area);
            cells.write(0, 0, text, CellStyle::new());
            cells.write(5, 0, family, CellStyle::new());
        }
        renderer.end_frame();
        assert_eq!(renderer.plain_text(), format!("{text}{family}\n"));

        renderer.begin_frame();
        {
            let mut cells = renderer.cells(area, area);
            cells.write(0, 0, text, CellStyle::new());
            cells.write(5, 0, family, CellStyle::new());
        }
        renderer.end_frame();
        assert!(renderer.output().is_empty());

        renderer.begin_frame();
        {
            let mut cells = renderer.cells(area, area);
            cells.write(0, 0, "界", CellStyle::new());
            cells.set_cell(1, 0, SurfaceCell::new('x'));
        }
        renderer.end_frame();
        assert_eq!(renderer.plain_text(), " x\n");

        renderer.begin_frame();
        {
            let mut cells = renderer.cells(area, area);
            cells.write(0, 0, "界", CellStyle::new());
            cells.set_cell(
                1,
                0,
                SurfaceCell::default().style(CellStyle::new().background(Color::RED)),
            );
        }
        renderer.end_frame();
        assert_eq!(renderer.plain_text(), "\n");
        assert_eq!(renderer.cells[1].background, Color::RED);

        renderer.begin_frame();
        renderer
            .cells(area, area)
            .write(7, 0, "界", CellStyle::new());
        renderer.end_frame();
        assert_eq!(renderer.plain_text(), "\n");

        renderer.begin_frame();
        renderer
            .cells(area, area)
            .set_cell(0, 0, SurfaceCell::new('界'));
        renderer.end_frame();
        assert_eq!(renderer.plain_text(), "界\n");

        renderer.begin_frame();
        renderer
            .cells(area, area)
            .set_cell(0, 0, SurfaceCell::new('a'));
        renderer.end_frame();
        assert_eq!(renderer.plain_text(), "a\n");
        assert!(String::from_utf8_lossy(renderer.output()).contains("a "));
    }

    #[test]
    fn text_uses_quantized_alignment_and_span_styles() {
        let mut renderer = renderer(7, 3);
        let spans = [
            Span::new("err")
                .color(Color::RED)
                .attributes(TextAttributes::BOLD),
            Span::new("or"),
        ];
        let text = renderer.rich_text(&spans);
        assert_eq!(renderer.rich_text(&spans), text);
        let area = LogicalRect::new(0.4, 0.4, 5.2, 3.0);
        let screen = renderer.screen().to_logical(SCALE);
        renderer.begin_frame();
        renderer.paint_text(
            TextRequest::new(text, area).color(Color::WHITE).options(
                TextOptions::new()
                    .horizontal_align(HorizontalAlign::Center)
                    .vertical_align(VerticalAlign::Center),
            ),
            screen,
        );
        renderer.end_frame();

        let start = renderer.columns;
        assert!(renderer.cells[start..start + 3].iter().all(|cell| {
            cell.foreground == Color::RED && cell.attributes == TextAttributes::BOLD
        }));
        assert!(renderer.cells[start + 3..start + 5].iter().all(|cell| {
            cell.foreground == Color::WHITE && cell.attributes == TextAttributes::NONE
        }));
    }

    #[test]
    fn text_cache_pins_presented_runs() {
        let mut renderer = renderer(1, 1);
        let area = renderer.screen().to_logical(SCALE);
        let text = renderer.text_run("x");
        renderer.begin_frame();
        renderer.paint_text(TextRequest::new(text, area), area);
        renderer.end_frame();

        renderer.text_run(&"y".repeat(TEXT_RUN_CACHE_CAPACITY));
        renderer.begin_frame();
        renderer.paint_text(TextRequest::new(text, area), area);
        renderer.end_frame();

        assert_eq!(renderer.plain_text(), "x\n");
    }

    #[test]
    fn word_wrap_and_layouts_are_cached() {
        let mut renderer = renderer(20, 4);
        let text = renderer.text_run("hello world");
        assert_eq!(renderer.text_run("hello world"), text);
        let request = TextLayoutRequest::new(text)
            .wrap(TextWrap::Word)
            .max_width(7.0);
        let layout = renderer.layout_text(&request);
        assert_eq!(renderer.layout_text(&request), layout);
        let layout = renderer.text_layouts.get_index(layout);
        assert_eq!(layout.lines.len(), 2);
        assert_eq!(layout.lines[0].width, 5);
        assert_eq!(layout.lines[1].width, 5);
    }

    #[test]
    fn disjoint_interaction_clip_is_empty() {
        let renderer = renderer(4, 3);
        assert_eq!(
            renderer.interaction_area(
                LogicalRect::new(0.0, 0.0, 1.0, 1.0),
                LogicalRect::new(3.0, 0.0, 1.0, 1.0),
            ),
            None
        );
    }
}
