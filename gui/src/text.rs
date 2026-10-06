use std::ops::Range;

use blit::geometry::LogicalRect;
pub use blit_text::FontStyle;

use crate::color::Color;

/// cached horizontal positions for grayscale glyph rasterization
///
/// more phases improve fractional text placement at the cost of glyph cache space
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum TextPhases {
    /// snaps glyphs to physical pixels
    One = 1,
    /// caches glyphs at half-pixel positions
    Two = 2,
    /// caches glyphs at quarter-pixel positions
    #[default]
    Four = 4,
}

blit::builder! {
    /// style overrides for a rich text span
    #[derive(Clone, Debug, PartialEq)]
    pub struct SpanStyle {
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
    }
}

blit::builder! {
    /// a styled byte range within a rich text string
    #[derive(Clone, Debug, PartialEq)]
    pub struct Span {
        new(range: Range<usize>),
        style: SpanStyle = SpanStyle::new(),
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
pub struct TextRequest {
    pub text: TextRunId,
    pub area: LogicalRect,
    pub offset_x: f32,
    pub color: Color,
    pub options: TextOptions,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLayoutRequest {
    pub text: TextRunId,
    pub wrap: TextWrap,
    pub max_width: Option<f32>,
    pub max_lines: Option<u16>,
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct TextStyle {
        new(),
        font: FontId = FontId(0),
        size: f32 = 16.0,
        weight: u16 = 400,
        stretch: u16 = 100,
        style: FontStyle = FontStyle::Normal,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextRunId(#[doc(hidden)] pub u64);
