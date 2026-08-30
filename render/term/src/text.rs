use crate::color::Color;
use blit::LogicalRect;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct TextRequest {
        new(text: TextRunId, area: LogicalRect),
        color: Color = Color::Reset,
        offset_x: f32 = 0.0,
        bold: bool = false,
        options: TextOptions = TextOptions::new(),
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct TextLayoutRequest {
        new(text: TextRunId),
        @optional {
            max_width: f32,
            max_lines: u16,
        },
        wrap: TextWrap = TextWrap::None,
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct TextOptions {
        new(),
        @optional {
            max_lines: u16,
        },
        wrap: TextWrap = TextWrap::None,
        overflow: TextOverflow = TextOverflow::Clip,
        horizontal_align: HorizontalAlign = HorizontalAlign::Left,
        vertical_align: VerticalAlign = VerticalAlign::Top,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextRunId(pub u64);
