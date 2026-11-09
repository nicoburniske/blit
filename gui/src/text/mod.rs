use std::{fmt, ops::Range};

use blit::{Atom, AtomCx, Constraints, FrameSlice, LogicalRect, Paint, Size, Widget, state};
pub use blit_text::{FontSettings, FontStyle, OpticalSize};

use crate::{Context, color::Color, paint::TextRequest};

mod system;

pub use system::{FontStack, GlyphImageId, ResolvedTextLayout, TextConfig, TextSystem};

blit::builder! {
    #[derive(Clone, Copy, Debug)]
    pub struct Text<'a> {
        new(#[into] text: TextSource<'a>),
        style: TextStyle<'a> = TextStyle::new(),
        offset_x: f32 = 0.0,
        options: TextOptions = TextOptions::new(),
    }
}

impl<C: Context + for<'a> Paint<TextRequest<'a>>> Widget<C, state::Node> for Text<'_> {
    type Response = ();

    fn build(self, mut ui: blit::Ui<'_, C, state::Node>) {
        let run = ui.context_mut().text().text_run(self.text, self.style);
        let colors = ui.store_slice(std::iter::once(self.style.color));
        ui.insert(TextAtom::new(run, colors).offset_x(self.offset_x).options(self.options));
    }
}

blit::builder! {
    /// text partitioned by ordered, contiguous spans
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct RichText<'a> {
        new(text: &'a str, spans: &'a [Span<'a>]),
        style: TextStyle<'a> = TextStyle::new(),
        offset_x: f32 = 0.0,
        options: TextOptions = TextOptions::new(),
    }
}

impl<C: Context + for<'a> Paint<TextRequest<'a>>> Widget<C, state::Node> for RichText<'_> {
    type Response = ();

    fn build(self, mut ui: blit::Ui<'_, C, state::Node>) {
        let run = ui.context_mut().text().rich_text(self.text, self.spans, self.style);
        // empty spans lay out as one span in the base color
        let colors = if self.spans.is_empty() {
            ui.store_slice(std::iter::once(self.style.color))
        } else {
            ui.store_slice(
                self.spans
                    .iter()
                    .map(|span| span.style.color.unwrap_or(self.style.color)),
            )
        };
        ui.insert(TextAtom::new(run, colors).offset_x(self.offset_x).options(self.options));
    }
}

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

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct TextAtom {
        // one color per span
        new(text: TextRun, colors: FrameSlice<Color>),
        offset_x: f32 = 0.0,
        options: TextOptions = TextOptions::new(),
    }
}

impl<C: Context + for<'a> Paint<TextRequest<'a>>> Atom<C> for TextAtom {
    fn measure(&self, cx: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        let measured = cx.context_mut().text().measure(&TextLayoutRequest {
            text: self.text,
            wrap: self.options.wrap,
            max_width: (self.options.wrap != TextWrap::None && constraints.max.width.is_finite())
                .then_some(constraints.max.width),
            max_lines: self.options.max_lines,
        });
        constraints.constrain(measured)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        let colors = cx.slice(self.colors);
        cx.context_mut().paint(
            area,
            TextRequest {
                text: self.text,
                colors,
                area,
                offset_x: self.offset_x,
                options: self.options,
            },
        );
    }
}
