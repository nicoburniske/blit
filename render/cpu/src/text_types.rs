use crate::color::Color;
use blit::geometry::LogicalRect;

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub font: FontId,
    pub size: f32,
    pub weight: u16,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: FontId::default(),
            size: 16.0,
            weight: 400,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontId(pub u16);

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextRunId(pub u64);
