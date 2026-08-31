use std::ops::{BitOr, BitOrAssign};

use crate::color::Color;
use blit::LogicalRect;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct Span<'a> {
        new(text: &'a str),
        @optional {
            color: Color,
        },
        attributes: TextAttributes = TextAttributes::NONE,
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextAttributes(u8);

impl TextAttributes {
    pub const NONE: Self = Self(0);
    pub const BOLD: Self = Self(1 << 0);
    pub const DIM: Self = Self(1 << 1);
    pub const ITALIC: Self = Self(1 << 2);
    pub const UNDERLINE: Self = Self(1 << 3);
    pub const BLINK: Self = Self(1 << 4);
    pub const INVERSE: Self = Self(1 << 5);
    pub const HIDDEN: Self = Self(1 << 6);
    pub const STRIKETHROUGH: Self = Self(1 << 7);

    pub const fn contains(self, attributes: Self) -> bool {
        self.0 & attributes.0 == attributes.0
    }

    pub fn set(&mut self, attributes: Self, enabled: bool) {
        if enabled {
            self.0 |= attributes.0;
        } else {
            self.0 &= !attributes.0;
        }
    }
}

impl BitOr for TextAttributes {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for TextAttributes {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct TextRequest {
        new(text: TextRunId, area: LogicalRect),
        color: Color = Color::Reset,
        offset_x: f32 = 0.0,
        attributes: TextAttributes = TextAttributes::NONE,
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
