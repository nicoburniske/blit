use tiny_skia::Color;

use crate::{Rect, Ui};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontId(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontWeight {
    #[default]
    Normal,
    Medium,
    Bold,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextWrap {
    #[default]
    None,
    Word,
    Character,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HorizontalAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub font: FontId,
    pub size: f32,
    pub weight: FontWeight,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: FontId::default(),
            size: 16.0,
            weight: FontWeight::Normal,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextOptions {
    pub wrap: TextWrap,
    pub overflow: TextOverflow,
    pub horizontal_align: HorizontalAlign,
    pub vertical_align: VerticalAlign,
    pub max_lines: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRequest<'a> {
    pub text: &'a str,
    pub area: Rect,
    pub color: Color,
    pub style: TextStyle,
    pub options: TextOptions,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub lines: u16,
}

pub struct Text<'a> {
    text: &'a str,
    area: Option<Rect>,
    position: Point,
    color: Color,
    style: TextStyle,
    options: TextOptions,
}

impl<'a> Text<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            area: None,
            position: Point::default(),
            color: Color::BLACK,
            style: TextStyle::default(),
            options: TextOptions::default(),
        }
    }
    pub fn at(mut self, position: Point) -> Self {
        self.position = position;
        self.area = None;
        self
    }

    pub fn in_area(mut self, area: Rect) -> Self {
        self.area = Some(area);
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn font(mut self, font: FontId) -> Self {
        self.style.font = font;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.style.size = size;
        self
    }

    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.style.weight = weight;
        self
    }

    pub fn wrap(mut self, wrap: TextWrap) -> Self {
        self.options.wrap = wrap;
        self
    }

    pub fn overflow(mut self, overflow: TextOverflow) -> Self {
        self.options.overflow = overflow;
        self
    }

    pub fn align(mut self, align: HorizontalAlign) -> Self {
        self.options.horizontal_align = align;
        self
    }

    pub fn vertical_align(mut self, align: VerticalAlign) -> Self {
        self.options.vertical_align = align;
        self
    }

    pub fn max_lines(mut self, max_lines: u16) -> Self {
        self.options.max_lines = Some(max_lines);
        self
    }

    pub fn render(self, ui: &mut Ui) {
        let mut request = self.request(ui.screen);
        let metrics = ui.platform.measure_text(&request);
        let bounds = if let Some(area) = self.area {
            let x = match self.options.horizontal_align {
                HorizontalAlign::Left => area.x,
                HorizontalAlign::Center => area.x + (area.width - metrics.width) / 2.0,
                HorizontalAlign::Right => area.x + area.width - metrics.width,
            };
            let y = match self.options.vertical_align {
                VerticalAlign::Top => area.y,
                VerticalAlign::Center => area.y + (area.height - metrics.height) / 2.0,
                VerticalAlign::Bottom => area.y + area.height - metrics.height,
            };
            Rect {
                x,
                y,
                width: metrics.width.min(area.width),
                height: metrics.height.min(area.height),
            }
        } else {
            request.area.width = metrics.width;
            request.area.height = metrics.height;
            request.area
        };
        let mut clips = [Rect::default(); 8];
        let mut clip_count = 0;
        for dirty in ui.dirty.regions() {
            if let Some(clip) = bounds.intersection(*dirty) {
                clips[clip_count] = clip;
                clip_count += 1;
            }
        }
        if clip_count != 0 {
            ui.platform
                .draw_text(&mut ui.pixels, &request, &clips[..clip_count]);
        }
    }

    fn request(&self, screen: Rect) -> TextRequest<'a> {
        TextRequest {
            text: self.text,
            area: self.area.unwrap_or(Rect {
                x: self.position.x,
                y: self.position.y,
                width: (screen.x + screen.width - self.position.x).max(0.0),
                height: (screen.y + screen.height - self.position.y).max(0.0),
            }),
            color: self.color,
            style: self.style,
            options: self.options,
        }
    }
}
