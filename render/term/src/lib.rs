use std::{fmt::Write as _, io};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use blit::{
    color::Color,
    command_list::{Command, CommandList},
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    image::{ImageData, ImageHandle, ImageId},
    renderer::Renderer,
    style::Border,
    text::{
        HorizontalAlign, TextLayoutRequest, TextOverflow, TextRequest, TextRunId, TextStyle,
        TextWrap, VerticalAlign,
    },
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const CELL_WIDTH: f32 = 8.0;
pub const CELL_HEIGHT: f32 = 16.0;

pub struct TerminalRenderer {
    columns: usize,
    rows: usize,
    scale_factor: f32,
    text_runs: Vec<(String, TextStyle)>,
    images: Vec<StoredImage>,
    kitty_placements: Vec<KittyPlacement>,
    presented_kitty_placements: Vec<KittyPlacement>,
    pixels: Vec<Pixel>,
    glyphs: Vec<Option<Glyph>>,
    boxes: Vec<BoxCell>,
    cells: Vec<Cell>,
    previous: Vec<Cell>,
    damaged: Vec<bool>,
}

impl TerminalRenderer {
    pub fn new(columns: u16, rows: u16) -> Self {
        let columns = usize::from(columns.max(1));
        let rows = usize::from(rows.max(1));
        Self {
            columns,
            rows,
            scale_factor: 1.0,
            text_runs: Vec::new(),
            images: Vec::new(),
            kitty_placements: Vec::new(),
            presented_kitty_placements: Vec::new(),
            pixels: vec![Pixel::default(); columns * 2 * rows * 2],
            glyphs: vec![None; columns * rows],
            boxes: vec![BoxCell::default(); columns * rows],
            cells: vec![Cell::default(); columns * rows],
            previous: vec![Cell::invalid(); columns * rows],
            damaged: vec![true; columns * rows],
        }
    }

    pub fn screen(&self) -> PhysicalRect {
        PhysicalRect {
            x: 0,
            y: 0,
            width: (self.columns as f32 * CELL_WIDTH) as i32,
            height: (self.rows as f32 * CELL_HEIGHT) as i32,
        }
    }

    pub fn resize(&mut self, columns: u16, rows: u16) {
        let columns = usize::from(columns.max(1));
        let rows = usize::from(rows.max(1));
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
        let cell_width = CELL_WIDTH / self.scale_factor;
        let cell_height = CELL_HEIGHT / self.scale_factor;
        (
            (area.x / cell_width)
                .round()
                .clamp(0.0, self.columns as f32) as usize,
            (area.y / cell_height).round().clamp(0.0, self.rows as f32) as usize,
            ((area.x + area.width) / cell_width)
                .round()
                .clamp(0.0, self.columns as f32) as usize,
            ((area.y + area.height) / cell_height)
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

    pub fn present(&mut self, output: &mut impl io::Write) -> io::Result<()> {
        let mut ansi = String::new();
        let mut style = None;
        for y in 0..self.rows {
            let mut x = 0;
            while x < self.columns {
                let index = y * self.columns + x;
                if !self.damaged[index] || self.previous[index] == self.cells[index] {
                    x += 1;
                    continue;
                }
                write!(ansi, "\x1b[{};{}H", y + 1, x + 1).unwrap();
                while x < self.columns {
                    let index = y * self.columns + x;
                    let cell = &self.cells[index];
                    if !self.damaged[index] || self.previous[index] == *cell {
                        break;
                    }
                    let next_style = (cell.foreground, cell.background, cell.bold);
                    if style != Some(next_style) {
                        write!(
                            ansi,
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
                    ansi.push_str(&cell.text);
                    self.previous[index] = cell.clone();
                    x += 1;
                }
            }
        }
        ansi.push_str("\x1b[0m");
        for placement in &self.presented_kitty_placements {
            if !self.kitty_placements.contains(placement) {
                write!(
                    ansi,
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
                            ansi,
                            "\x1b_Ga=t,f=32,s={},v={},i={},m={more},q=2;",
                            image.width, image.height, placement.image
                        )
                        .unwrap();
                    } else {
                        write!(ansi, "\x1b_Gm={more},q=2;").unwrap();
                    }
                    BASE64.encode_string(chunk, &mut ansi);
                    ansi.push_str("\x1b\\");
                }
                image.transmitted = true;
            }
            if !self.presented_kitty_placements.contains(placement) {
                write!(
                    ansi,
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
        output.write_all(ansi.as_bytes())?;
        output.flush()
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
                output.push_str(&cell.text);
            }
            while output.ends_with(' ') {
                output.pop();
            }
            output.push('\n');
        }
        output
    }

    pub fn write_svg(&self, mut output: impl io::Write) -> io::Result<()> {
        let width = self.columns * 9;
        let height = self.rows * 18;
        write!(
            output,
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">\n<rect width=\"100%\" height=\"100%\" fill=\"#000\"/>\n"
        )?;
        for (index, cell) in self.cells.iter().enumerate() {
            let x = index % self.columns * 9;
            let y = index / self.columns * 18;
            writeln!(
                output,
                "<rect x=\"{x}\" y=\"{y}\" width=\"9\" height=\"18\" fill=\"#{:02x}{:02x}{:02x}\"/>",
                cell.background.red, cell.background.green, cell.background.blue
            )?;
        }
        for placement in &self.kitty_placements {
            let image = &self.images[placement.image as usize - 1];
            let left = placement.x as f32 * 9.0;
            let top = placement.y as f32 * 18.0;
            let pixel_width = placement.width as f32 * 9.0 / image.width as f32;
            let pixel_height = placement.height as f32 * 18.0 / image.height as f32;
            for y in 0..image.height {
                for x in 0..image.width {
                    let offset = (y * image.width + x) * 4;
                    writeln!(
                        output,
                        "<rect x=\"{}\" y=\"{}\" width=\"{pixel_width}\" height=\"{pixel_height}\" fill=\"#{:02x}{:02x}{:02x}\" fill-opacity=\"{}\"/>",
                        left + x as f32 * pixel_width,
                        top + y as f32 * pixel_height,
                        image.rgba[offset],
                        image.rgba[offset + 1],
                        image.rgba[offset + 2],
                        image.rgba[offset + 3] as f32 / 255.0,
                    )?;
                }
            }
        }
        for (index, cell) in self.cells.iter().enumerate() {
            let x = index % self.columns * 9;
            let y = index / self.columns * 18;
            if cell.text != " " {
                let text = cell
                    .text
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('"', "&quot;");
                writeln!(
                    output,
                    "<text x=\"{x}\" y=\"{}\" fill=\"#{:02x}{:02x}{:02x}\" font-family=\"monospace\" font-size=\"15\"{}>{text}</text>",
                    y + 15,
                    cell.foreground.red,
                    cell.foreground.green,
                    cell.foreground.blue,
                    if cell.bold {
                        " font-weight=\"bold\""
                    } else {
                        ""
                    }
                )?;
            }
        }
        output.write_all(b"</svg>\n")
    }

    fn layout_text(&self, request: &TextLayoutRequest) -> TextLayout {
        fn start_line(lines: &mut Vec<Line>, limit: usize) -> bool {
            if lines.len() >= limit {
                return false;
            }
            lines.push(Line::default());
            true
        }

        let text = &self.text_runs[request.text.0 as usize - 1].0;
        let cell_width = CELL_WIDTH / self.scale_factor;
        let line_height = CELL_HEIGHT / self.scale_factor;
        let max_columns = request
            .max_width
            .map(|width| (width / cell_width).floor().max(0.0) as usize);
        let max_lines = usize::from(request.max_lines.unwrap_or(u16::MAX)).max(1);
        let mut lines = vec![Line::default()];
        let mut truncated = false;
        match request.wrap {
            TextWrap::Word => {
                'tokens: for token in text.split_word_bounds() {
                    let whitespace = token.chars().all(char::is_whitespace);
                    let token_width = UnicodeWidthStr::width(token);
                    if !whitespace
                        && lines.last().unwrap().width != 0
                        && max_columns.is_some_and(|maximum| {
                            lines.last().unwrap().width + token_width > maximum
                        })
                    {
                        let current = lines.last_mut().unwrap();
                        while current
                            .graphemes
                            .last()
                            .is_some_and(|grapheme| grapheme.chars().all(char::is_whitespace))
                        {
                            let grapheme = current.graphemes.pop().unwrap();
                            current.width -= UnicodeWidthStr::width(grapheme.as_str()).max(1);
                        }
                        if !start_line(&mut lines, max_lines) {
                            truncated = true;
                            break;
                        }
                    }
                    for grapheme in token.graphemes(true) {
                        if grapheme == "\n" || grapheme == "\r\n" {
                            if !start_line(&mut lines, max_lines) {
                                truncated = true;
                                break 'tokens;
                            }
                            continue;
                        }
                        let width = UnicodeWidthStr::width(grapheme).max(1);
                        if max_columns
                            .is_some_and(|maximum| lines.last().unwrap().width + width > maximum)
                            && lines.last().unwrap().width != 0
                        {
                            if !start_line(&mut lines, max_lines) {
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
                        let current = lines.last_mut().unwrap();
                        current.graphemes.push(grapheme.to_owned());
                        current.width += width;
                    }
                }
            }
            TextWrap::None | TextWrap::Character => {
                'graphemes: for grapheme in text.graphemes(true) {
                    if grapheme == "\n" || grapheme == "\r\n" {
                        if !start_line(&mut lines, max_lines) {
                            truncated = true;
                            break;
                        }
                        continue;
                    }
                    let width = UnicodeWidthStr::width(grapheme).max(1);
                    if request.wrap == TextWrap::Character
                        && max_columns
                            .is_some_and(|maximum| lines.last().unwrap().width + width > maximum)
                        && lines.last().unwrap().width != 0
                        && !start_line(&mut lines, max_lines)
                    {
                        truncated = true;
                        break 'graphemes;
                    }
                    let current = lines.last_mut().unwrap();
                    current.graphemes.push(grapheme.to_owned());
                    current.width += width;
                }
            }
        }
        TextLayout {
            width: lines.iter().map(|line| line.width).max().unwrap_or(0) as f32 * cell_width,
            height: lines.len() as f32 * line_height,
            lines,
            truncated,
        }
    }
}

impl Renderer for TerminalRenderer {
    fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor;
    }

    fn layout_resolution(&self) -> blit::layout::LayoutResolution {
        blit::layout::LayoutResolution::Discrete {
            step: LogicalSize {
                width: CELL_WIDTH / self.scale_factor,
                height: CELL_HEIGHT / self.scale_factor,
            },
        }
    }

    fn interaction_area(&self, area: LogicalRect, clip: LogicalRect) -> Option<LogicalRect> {
        let cell_width = CELL_WIDTH / self.scale_factor;
        let cell_height = CELL_HEIGHT / self.scale_factor;
        let (mut left, mut top, mut right, mut bottom) = self.cell_bounds(area);
        left = left.max(
            (clip.x / cell_width - 0.5)
                .ceil()
                .clamp(0.0, self.columns as f32) as usize,
        );
        top = top.max(
            (clip.y / cell_height - 0.5)
                .ceil()
                .clamp(0.0, self.rows as f32) as usize,
        );
        right = right.min(
            ((clip.x + clip.width) / cell_width - 0.5)
                .ceil()
                .clamp(0.0, self.columns as f32) as usize,
        );
        bottom = bottom.min(
            ((clip.y + clip.height) / cell_height - 0.5)
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
        self.damaged.fill(false);
        let screen = self.screen();
        let mut damage_bounds = (self.columns, self.rows, 0, 0);
        for damage in damage {
            let Some(damage) = damage.intersection(screen) else {
                continue;
            };
            let left = (damage.x as f32 / CELL_WIDTH).floor() as usize;
            let top = (damage.y as f32 / CELL_HEIGHT).floor() as usize;
            let right = (damage.x.saturating_add(damage.width) as f32 / CELL_WIDTH)
                .ceil()
                .min(self.columns as f32) as usize;
            let bottom = (damage.y.saturating_add(damage.height) as f32 / CELL_HEIGHT)
                .ceil()
                .min(self.rows as f32) as usize;
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
            return;
        }
        self.clear_damaged(damage_bounds);
        self.kitty_placements.clear();
        let logical_screen = screen.to_logical(self.scale_factor);
        for (z, record) in commands.iter().enumerate() {
            if !matches!(record.command, Command::Image(_)) {
                let Some(bounds) = record.bounds.intersection(screen) else {
                    continue;
                };
                let left = (bounds.x as f32 / CELL_WIDTH).floor() as usize;
                let top = (bounds.y as f32 / CELL_HEIGHT).floor() as usize;
                let right =
                    (bounds.x.saturating_add(bounds.width) as f32 / CELL_WIDTH).ceil() as usize;
                let bottom =
                    (bounds.y.saturating_add(bounds.height) as f32 / CELL_HEIGHT).ceil() as usize;
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
                    let cell_width = CELL_WIDTH / self.scale_factor;
                    let cell_height = CELL_HEIGHT / self.scale_factor;
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
                                    cell.rounded = cell.edges == 0 && rounded;
                                    cell.edges |= edges;
                                    cell.color = color;
                                    cell.z = cell.z.max(z);
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
                                    cell.rounded = false;
                                    cell.edges |= 1 | 4;
                                    cell.color = color;
                                    cell.z = cell.z.max(z);
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
                    let mut layout = self.layout_text(&layout_request);
                    if request.options.overflow == TextOverflow::Ellipsis
                        && (layout.truncated || layout.width > request.area.width)
                    {
                        let maximum = (request.area.width / (CELL_WIDTH / self.scale_factor))
                            .floor()
                            .max(1.0) as usize;
                        if let Some(line) = layout.lines.first_mut() {
                            while line.width >= maximum && !line.graphemes.is_empty() {
                                let removed = line.graphemes.pop().unwrap();
                                line.width -= UnicodeWidthStr::width(removed.as_str()).max(1);
                            }
                            line.graphemes.push("…".into());
                            line.width += 1;
                        }
                    }
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
                        let start_x = match request.options.horizontal_align {
                            HorizontalAlign::Left => {
                                area_left as isize
                                    - (request.offset_x / (CELL_WIDTH / self.scale_factor)).round()
                                        as isize
                            }
                            HorizontalAlign::Center => {
                                area_left as isize
                                    + (area_width - line.width as isize).div_euclid(2)
                            }
                            HorizontalAlign::Right => area_right as isize - line.width as isize,
                        };
                        let mut column = 0;
                        for grapheme in &line.graphemes {
                            let width = UnicodeWidthStr::width(grapheme.as_str()).max(1);
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
                                        text: grapheme.clone(),
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
                                            text: String::new(),
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
                        let cell_width = CELL_WIDTH / self.scale_factor;
                        let cell_height = CELL_HEIGHT / self.scale_factor;
                        let x = (area.x / cell_width).floor().max(0.0) as usize;
                        let y = (area.y / cell_height).floor().max(0.0) as usize;
                        let right = ((area.x + area.width) / cell_width)
                            .ceil()
                            .min(self.columns as f32) as usize;
                        let bottom = ((area.y + area.height) / cell_height)
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
                    self.cells[index] = Cell {
                        text: glyph.text.clone(),
                        foreground: glyph.color,
                        background,
                        bold: glyph.bold,
                    };
                    continue;
                }
                if box_cell.edges != 0 && pixel_z <= box_cell.z {
                    self.cells[index] = Cell {
                        text: if box_cell.rounded {
                            match box_cell.edges {
                                3 => "╰",
                                6 => "╭",
                                9 => "╯",
                                12 => "╮",
                                _ => BOXES[box_cell.edges as usize],
                            }
                        } else {
                            BOXES[box_cell.edges as usize]
                        }
                        .into(),
                        foreground: box_cell.color,
                        background,
                        bold: false,
                    };
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
                self.cells[index] = Cell {
                    text: QUADRANTS[mask].into(),
                    foreground,
                    background,
                    bold: false,
                };
            }
        }
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
        if let Some(index) = self
            .text_runs
            .iter()
            .position(|(stored, stored_style)| stored == text && *stored_style == style)
        {
            return TextRunId(index as u64 + 1);
        }
        self.text_runs.push((text.to_owned(), style));
        TextRunId(self.text_runs.len() as u64)
    }

    fn text_offset_at_position(&mut self, request: &TextRequest, position: LogicalPoint) -> usize {
        let text = &self.text_runs[request.text.0 as usize - 1].0;
        let target = ((position.x - request.area.x + request.offset_x)
            / (CELL_WIDTH / self.scale_factor))
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
        LogicalSize {
            width: layout.width,
            height: layout.height,
        }
    }

    fn text_cursor_rect(&mut self, request: &TextRequest, byte_offset: usize) -> LogicalRect {
        let text = &self.text_runs[request.text.0 as usize - 1].0;
        let before = &text[..text.floor_char_boundary(byte_offset.min(text.len()))];
        let line = before.rsplit_once('\n').map_or(before, |(_, line)| line);
        LogicalRect {
            x: request.area.x
                + UnicodeWidthStr::width(line) as f32 * CELL_WIDTH / self.scale_factor
                - request.offset_x,
            y: request.area.y
                + before.matches('\n').count() as f32 * CELL_HEIGHT / self.scale_factor,
            width: 0.0,
            height: CELL_HEIGHT / self.scale_factor,
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

#[derive(Clone)]
struct Glyph {
    text: String,
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

#[derive(Default)]
struct Line {
    graphemes: Vec<String>,
    width: usize,
}

struct TextLayout {
    lines: Vec<Line>,
    width: f32,
    height: f32,
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

    #[test]
    fn real_blit_text_reaches_terminal_cells() {
        let mut renderer = TerminalRenderer::new(20, 4);
        let mut state = UiState::new(renderer.screen(), 1.0);
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
    fn absolute_slots_use_terminal_resolution() {
        use blit::{container::Absolute, interact::WidgetId, layout::Flex};

        let mut renderer = TerminalRenderer::new(4, 3);
        let mut state = UiState::new(renderer.screen(), 1.0);
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

        let mut renderer = TerminalRenderer::new(4, 3);
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
            area.to_physical(1.0),
            ClipId::default(),
        );
        renderer.render(&commands, &[renderer.screen()]);

        assert_eq!(renderer.cells[renderer.columns + 1].text, "x");
    }

    #[test]
    fn text_alignment_uses_quantized_area() {
        use blit::{command_list::ClipId, text::TextOptions};

        let mut renderer = TerminalRenderer::new(5, 5);
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
            area.to_physical(1.0),
            ClipId::default(),
        );
        renderer.render(&commands, &[renderer.screen()]);

        assert_eq!(renderer.cells[renderer.columns + 1].text, "x");
    }

    #[test]
    fn damaged_render_matches_full_render() {
        use blit::command_list::{ClipId, Rectangle};

        let screen = TerminalRenderer::new(8, 4).screen();
        let frame = |x: f32| {
            let mut commands = CommandList::default();
            commands.push_clear(screen);
            let background = screen.to_logical(1.0);
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
                accent.to_physical(1.0),
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
            .to_physical(1.0),
            LogicalRect {
                x: CELL_WIDTH * 2.0,
                y: CELL_HEIGHT,
                width: CELL_WIDTH,
                height: CELL_HEIGHT,
            }
            .to_physical(1.0),
        ];

        let mut incremental = TerminalRenderer::new(8, 4);
        incremental.render(&old, &[screen]);
        incremental.render(&current, &damage);
        let mut full = TerminalRenderer::new(8, 4);
        full.render(&current, &[screen]);

        assert_eq!(incremental.cells, full.cells);
    }

    #[test]
    fn word_wrap_keeps_words_intact() {
        let mut renderer = TerminalRenderer::new(20, 4);
        let text = renderer.text_run("hello world", TextStyle::default());
        let layout = renderer.layout_text(&TextLayoutRequest {
            text,
            style: TextStyle::default(),
            wrap: TextWrap::Word,
            max_width: Some(CELL_WIDTH * 7.0),
            max_lines: None,
        });
        assert_eq!(layout.lines.len(), 2);
        assert_eq!(layout.lines[0].width, 5);
        assert_eq!(layout.lines[1].width, 5);
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
        let mut renderer = TerminalRenderer::new(8, 4);
        let mut state = UiState::new(renderer.screen(), 1.0);
        state.set_scale_factor(1.25);
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

        let mut renderer = TerminalRenderer::new(4, 3);
        let mut state = UiState::new(renderer.screen(), 1.0);
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
        let renderer = TerminalRenderer::new(4, 3);
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

        let mut renderer = TerminalRenderer::new(8, 4);
        let image = renderer.create_image(ImageData::new(
            ImagePixels::Static(&[255, 0, 0, 255]),
            ImageFormat::Rgba8,
            1,
            1,
        ));
        let mut state = UiState::new(renderer.screen(), 1.0);
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
        let mut output = Vec::new();
        renderer.present(&mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("\x1b_Ga=t,f=32"));
        assert!(output.contains("\x1b_Ga=p"));
    }
}
