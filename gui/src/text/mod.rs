use std::{
    fmt,
    hash::{Hash, Hasher},
    ops::Range,
};

use blit::{Atom, AtomCx, Constraints, FrameSlice, HasFrameStore, LogicalRect, Paint, Size, Widget, state};
use blit_text::{FontSettings, FontStyle};

use crate::{
    Context,
    color::Color,
    display_list::{Compare, DisplayList},
};

pub mod system;

blit::builder! {
    #[derive(Clone, Copy, Debug)]
    pub struct Text<'a> {
        new(#[into] text: TextSource<'a>),
        style: TextStyle<'a> = TextStyle::new(),
        offset_x: f32 = 0.0,
        options: TextOptions = TextOptions::new(),
    }
}

impl<C> Widget<C, state::Node> for Text<'_>
where
    C: Context + Paint<TextAtom>,
{
    type Response = ();

    fn build(self, mut ui: blit::Ui<'_, C, state::Node>) {
        let run = ui.context_mut().text().text_run(self.text, self.style);
        let mut colors = ui.store_mut().slice(1);
        colors.push(self.style.color);
        let colors = colors.finish();
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

impl<C> Widget<C, state::Node> for RichText<'_>
where
    C: Context + Paint<TextAtom>,
{
    type Response = ();

    fn build(self, mut ui: blit::Ui<'_, C, state::Node>) {
        let run = ui.context_mut().text().rich_text(self.text, self.spans, self.style);
        // empty spans lay out as one span in the base color
        let mut colors = ui.store_mut().slice(self.spans.len().max(1));
        if self.spans.is_empty() {
            colors.push(self.style.color);
        } else {
            colors.extend(
                self.spans
                    .iter()
                    .map(|span| span.style.color.unwrap_or(self.style.color)),
            )
        }
        let colors = colors.finish();
        ui.insert(TextAtom::new(run, colors).offset_x(self.offset_x).options(self.options));
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRequest<'a> {
    pub text: TextRun,
    /// one color per span, missing spans repeat the last color
    pub colors: &'a [Color],
    pub offset_x: f32,
    pub options: TextOptions,
}

impl TextRequest<'_> {
    /// stores borrowed colors in the context and paints the result
    pub fn paint<C: HasFrameStore + Paint<TextAtom, Scalar = f32>>(self, area: LogicalRect, context: &mut C) {
        let mut colors = context.store_mut().slice(self.colors.len());
        colors.extend(self.colors.iter().copied());
        let text = TextAtom {
            text: self.text,
            colors: colors.finish(),
            offset_x: self.offset_x,
            options: self.options,
        };
        context.paint(area, text);
    }

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
        (if self.offset_x == 0.0 {
            0
        } else {
            self.offset_x.to_bits()
        })
        .hash(state);
        self.options.wrap.hash(state);
        self.options.overflow.hash(state);
        self.options.horizontal_align.hash(state);
        self.options.vertical_align.hash(state);
        self.options.max_lines.hash(state);
    }
}

impl TextAtom {
    #[inline]
    pub fn view<'a>(&self, list: &'a DisplayList) -> TextRequest<'a> {
        TextRequest {
            colors: list.store().view(self.colors),
            text: self.text,
            offset_x: self.offset_x,
            options: self.options,
        }
    }
}

impl Compare for TextAtom {
    #[inline]
    fn equal(a: &Self, left: &DisplayList, b: &Self, right: &DisplayList) -> bool {
        a.view(left) == b.view(right)
    }

    fn hash(value: &Self, list: &DisplayList, state: &mut impl Hasher) {
        value.view(list).hash(state);
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
    /// resolved text with span colors stored in the frame
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct TextAtom {
        // one color per span
        new(text: TextRun, colors: FrameSlice<Color>),
        offset_x: f32 = 0.0,
        options: TextOptions = TextOptions::new(),
    }
}

impl<C> Atom<C> for TextAtom
where
    C: Context + Paint<Self>,
{
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
        cx.context_mut().paint(area, *self);
    }
}
