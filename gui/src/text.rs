use std::{
    fmt,
    hash::{Hash, Hasher},
    ops::Range,
};

use blit::geometry::LogicalRect;
pub use blit_text::{FontSettings, FontStyle, OpticalSize};

use crate::color::Color;

/// cached text geometry
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextRun(pub(crate) u64);

/// text from borrowed strings or formatting arguments
#[derive(Clone, Copy, Debug)]
pub enum TextSource<'a> {
    Borrowed(&'a str),
    Formatted(fmt::Arguments<'a>),
}

impl<'a, T: AsRef<str> + ?Sized> From<&'a T> for TextSource<'a> {
    fn from(text: &'a T) -> Self {
        Self::Borrowed(T::as_ref(text))
    }
}

impl<'a, T: AsRef<str> + ?Sized> From<&'a mut T> for TextSource<'a> {
    fn from(text: &'a mut T) -> Self {
        Self::Borrowed(T::as_ref(text))
    }
}

impl<'a> From<fmt::Arguments<'a>> for TextSource<'a> {
    fn from(arguments: fmt::Arguments<'a>) -> Self {
        match arguments.as_str() {
            Some(text) => Self::Borrowed(text),
            None => Self::Formatted(arguments),
        }
    }
}

blit::builder! {
    /// style overrides for a rich text span
    #[derive(Clone, Debug, PartialEq)]
    pub struct SpanStyle<'a> {
        new(),
        #[into]
        font: Option<FontId> = None,
        #[into]
        size: Option<f32> = None,
        #[into]
        weight: Option<u16> = None,
        #[into]
        stretch: Option<u16> = None,
        #[into]
        style: Option<FontStyle> = None,
        #[into]
        color: Option<Color> = None,
        #[into]
        settings: Option<FontSettings<'a>> = None,
    }
}

blit::builder! {
    /// a styled byte range within a rich text string
    #[derive(Clone, Debug, PartialEq)]
    pub struct Span<'a> {
        new(range: Range<usize>),
        style: SpanStyle<'a> = SpanStyle::new(),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextWrap {
    #[default]
    None,
    Word,
    Character,
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
pub struct TextRequest<'a> {
    pub text: TextRun,
    /// one color per span, missing spans repeat the last color
    pub colors: &'a [Color],
    pub area: LogicalRect,
    pub offset_x: f32,
    pub options: TextOptions,
}

impl TextRequest<'_> {
    pub fn span_color(&self, span: usize) -> Color {
        self.colors
            .get(span)
            .or(self.colors.last())
            .copied()
            .unwrap_or(Color::TRANSPARENT)
    }
}

impl Hash for TextRequest<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.text.hash(state);
        self.colors.hash(state);
        [
            self.area.x,
            self.area.y,
            self.area.width,
            self.area.height,
            self.offset_x,
        ]
        .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
        .hash(state);
        self.options.wrap.hash(state);
        self.options.overflow.hash(state);
        self.options.horizontal_align.hash(state);
        self.options.vertical_align.hash(state);
        self.options.max_lines.hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLayoutRequest {
    pub text: TextRun,
    pub wrap: TextWrap,
    pub max_width: Option<f32>,
    pub max_lines: Option<u16>,
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct TextStyle<'a> {
        new(),
        font: FontId = FontId(0),
        size: f32 = 16.0,
        weight: u16 = 400,
        stretch: u16 = 100,
        style: FontStyle = FontStyle::Normal,
        color: Color = Color::BLACK,
        settings: FontSettings<'a> = FontSettings::new(),
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct TextOptions {
        new(),
        wrap: TextWrap = TextWrap::None,
        overflow: TextOverflow = TextOverflow::Clip,
        horizontal_align: HorizontalAlign = HorizontalAlign::Left,
        vertical_align: VerticalAlign = VerticalAlign::Top,
        #[into]
        max_lines: Option<u16> = None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontId(pub u16);
