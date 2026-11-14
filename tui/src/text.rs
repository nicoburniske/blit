use std::{
    fmt,
    ops::{BitOr, BitOrAssign},
};

use blit::{Atom, AtomCx, Constraints, Paint, PhysicalRect, Size, Widget, state};

use crate::{Context, color::Color};

//
// widgets
//

blit::builder! {
    /// text from borrowed strings or formatting arguments
    #[derive(Clone, Copy, Debug)]
    pub struct Text<'a> {
        new(#[into] text: TextSource<'a>),
        color: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
        options: TextOptions = TextOptions::new(),
        #[into]
        spans: Option<&'a [Span<'a>]> = None,
    }
}

impl<'a> Text<'a> {
    pub fn rich(spans: &'a [Span<'a>]) -> Self {
        Self::new("").spans(spans)
    }
}

impl<C> Widget<C, state::Node> for Text<'_>
where
    C: Context + Paint<FrameText>,
{
    type Response = ();

    fn build(self, mut ui: blit::Ui<'_, C, state::Node>) {
        let run = if let Some(spans) = self.spans {
            ui.context_mut().as_mut().renderer.rich_text(spans)
        } else {
            let renderer = &mut ui.context_mut().as_mut().renderer;
            match self.text {
                TextSource::Borrowed(text) => renderer.text_run(text),
                TextSource::Formatted(arguments) => renderer.text_run_fmt(arguments),
            }
        };
        ui.insert(
            FrameText::new(run)
                .color(self.color)
                .attributes(self.attributes)
                .options(self.options),
        );
    }
}

//
// painting
//

blit::builder! {
    /// resolved text painted into terminal cells
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct FrameText {
        new(text: TextRunId),
        color: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
        offset_x: i32 = 0,
        options: TextOptions = TextOptions::new(),
    }
}

impl<C> Atom<C> for FrameText
where
    C: Context + Paint<Self>,
{
    fn measure(&self, cx: &mut AtomCx<'_, C>, constraints: Constraints<i32>) -> Size<i32> {
        let measured = cx.context_mut().as_mut().renderer.measure_text(&TextLayoutRequest {
            text: self.text,
            wrap: self.options.wrap,
            max_width: (self.options.wrap != TextWrap::None && constraints.max.width != i32::MAX)
                .then_some(constraints.max.width),
            max_lines: self.options.max_lines,
        });
        constraints.constrain(measured)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: PhysicalRect) {
        cx.context_mut().paint(area, *self);
    }
}

impl FrameText {
    /// placement used for text queries
    pub fn request(&self) -> TextRequest {
        TextRequest {
            text: self.text,
            offset_x: self.offset_x,
            options: self.options,
        }
    }
}

//
// text system
//

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextRunId(pub u64);

/// text from borrowed strings or formatting arguments
#[derive(Clone, Copy, Debug)]
pub enum TextSource<'a> {
    Borrowed(&'a str),
    Formatted(fmt::Arguments<'a>),
}

impl<'a, T> From<&'a T> for TextSource<'a>
where
    T: AsRef<str> + ?Sized,
{
    fn from(text: &'a T) -> Self {
        Self::Borrowed(T::as_ref(text))
    }
}

impl<'a, T> From<&'a mut T> for TextSource<'a>
where
    T: AsRef<str> + ?Sized,
{
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

/// placed text for queries such as cursor and hit testing
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRequest {
    pub text: TextRunId,
    pub offset_x: i32,
    pub options: TextOptions,
}

/// unplaced text for measuring
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLayoutRequest {
    pub text: TextRunId,
    pub wrap: TextWrap,
    pub max_width: Option<i32>,
    pub max_lines: Option<u16>,
}

//
// style
//

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct Span<'a> {
        new(text: &'a str),
        #[into]
        color: Option<Color> = None,
        #[into]
        background: Option<Color> = None,
        attributes: TextAttributes = TextAttributes::NONE,
        remove_attributes: TextAttributes = TextAttributes::NONE,
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextAttributes(pub u16);

impl TextAttributes {
    pub const NONE: Self = Self(0);
    pub const BOLD: Self = Self(1 << 0);
    pub const DIM: Self = Self(1 << 1);
    pub const ITALIC: Self = Self(1 << 2);
    pub const UNDERLINE: Self = Self(1 << 3);
    pub const SLOW_BLINK: Self = Self(1 << 4);
    pub const BLINK: Self = Self::SLOW_BLINK;
    pub const RAPID_BLINK: Self = Self(1 << 5);
    pub const INVERSE: Self = Self(1 << 6);
    pub const HIDDEN: Self = Self(1 << 7);
    pub const STRIKETHROUGH: Self = Self(1 << 8);

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

//
// options
//

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct TextOptions {
        new(),
        #[into]
        max_lines: Option<u16> = None,
        wrap: TextWrap = TextWrap::None,
        overflow: TextOverflow = TextOverflow::Clip,
        horizontal_align: HorizontalAlign = HorizontalAlign::Left,
        vertical_align: VerticalAlign = VerticalAlign::Top,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
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
