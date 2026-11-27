use alloc::{borrow::Cow, vec::Vec};
use core::{
    fmt,
    hash::{Hash, Hasher},
    ops::Range,
};

use blit::{FrameSlice, LogicalPoint, LogicalRect, LogicalSize, Paint, PhysicalPoint, Widget, state};

use crate::{
    Context,
    color::Color,
    display_list::{Compare, DisplayList},
};

//
// text system
//

pub trait TextSystem: 'static {
    /// registers geometry using the font attributes in style
    /// returned text runs remain valid until finish_frame
    fn text_run(&mut self, text: &str, style: TextStyle) -> TextRun {
        self.rich_text(text, &[], style)
    }

    /// registers geometry using span ranges and font attributes
    /// returned text runs remain valid until finish_frame
    fn rich_text(&mut self, text: &str, spans: &[Span<'_>], style: TextStyle) -> TextRun;

    fn measure(&mut self, text: TextRun, max_width: Option<f32>, options: TextOptions) -> LogicalSize;

    fn offset_at_position(&mut self, area: LogicalRect, request: &TextRequest, position: LogicalPoint) -> usize;

    fn cursor_rect(&mut self, area: LogicalRect, request: &TextRequest, byte_offset: usize, width: f32)
    -> LogicalRect;

    /// appends positioned glyphs for painting
    /// returned glyph keys remain valid for rasterize until finish_frame
    fn resolve(&mut self, area: LogicalRect, request: &TextRequest, scale: f32, glyphs: &mut Vec<PositionedGlyph>);

    /// equal keys must produce identical pixels and bearings
    fn rasterize(&mut self, key: GlyphKey) -> GlyphImage;

    fn finish_frame(&mut self) {}
}

#[derive(Clone, Copy, Debug)]
pub struct PositionedGlyph {
    pub key: GlyphKey,
    pub origin: PhysicalPoint,
    pub span: u32,
}

/// glyph identity for renderer pixel caches
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphKey {
    pub font: u64,
    pub size: u32,
    pub glyph: u16,
    pub phase: u8,
}

impl Hash for GlyphKey {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        let packed = (u64::from(self.size) << 24) | (u64::from(self.glyph) << 8) | u64::from(self.phase);
        state.write_u128((u128::from(self.font) << 64) | u128::from(packed));
    }
}

#[derive(Debug, Default)]
pub struct GlyphImage {
    pub metrics: GlyphMetrics,
    pub format: GlyphFormat,
    pub pixels: Cow<'static, [u8]>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlyphFormat {
    /// tightly packed coverage tinted by the text color
    #[default]
    Alpha8,
    /// tightly packed rgba with encoded srgb channels multiplied by alpha
    /// keeps intrinsic colors and receives text alpha once
    Rgba8,
}

impl GlyphFormat {
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Alpha8 => 1,
            Self::Rgba8 => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphMetrics {
    pub width: u16,
    pub height: u16,
    /// pixel offset from the glyph origin including any horizontal subpixel phase
    pub left: i16,
    /// pixel offset from the baseline with positive values pointing down
    pub top: i16,
}

//
// widgets
//

blit::builder! {
    #[derive(Clone, Copy, Debug)]
    pub struct Text<'a> {
        new(#[into] text: TextSource<'a>),
        style: TextStyle<'a> = TextStyle::new(),
        offset: LogicalPoint = LogicalPoint::new(0.0, 0.0),
        options: TextOptions = TextOptions::new(),
    }
}

impl<C> Widget<C, state::Insert> for Text<'_>
where
    C: Context + Paint<FrameText>,
{
    type Response = ();

    fn build(self, mut ui: blit::Ui<'_, C, state::Insert>) {
        let run = match self.text {
            TextSource::Borrowed(text) => ui.context_mut().text().text_run(text, self.style),
            TextSource::Formatted(arguments) => {
                let text = ui.scratch().fmt(arguments);
                ui.context_mut().text().text_run(&text, self.style)
            }
        };
        let colors = ui.store_mut().slice_from(&[self.style.color]);
        ui.insert(
            FrameText::new(run, colors)
                .offset(self.offset)
                .options(self.options),
        );
    }
}

blit::builder! {
    /// text partitioned by ordered, contiguous spans
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct RichText<'a> {
        new(text: &'a str, spans: &'a [Span<'a>]),
        style: TextStyle<'a> = TextStyle::new(),
        offset: LogicalPoint = LogicalPoint::new(0.0, 0.0),
        options: TextOptions = TextOptions::new(),
    }
}

impl<C> Widget<C, state::Insert> for RichText<'_>
where
    C: Context + Paint<FrameText>,
{
    type Response = ();

    fn build(self, mut ui: blit::Ui<'_, C, state::Insert>) {
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
        ui.insert(
            FrameText::new(run, colors)
                .offset(self.offset)
                .options(self.options),
        );
    }
}

//
// painting
//

blit::builder! {
    /// resolved text with span colors stored in the frame
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct FrameText {
        // one color per span
        new(text: TextRun, colors: FrameSlice<Color>),
        offset: LogicalPoint = LogicalPoint::new(0.0, 0.0),
        options: TextOptions = TextOptions::new(),
    }
}

impl<C> Widget<C, state::Insert> for FrameText
where
    C: Context + Paint<Self>,
{
    type Response = ();

    fn build(self, mut ui: blit::Ui<'_, C, state::Insert>) {
        let text = self.text;
        let options = self.options;
        ui.measure(move |cx, constraints| {
            let max_width = (options.wrap != TextWrap::None && constraints.max.width.is_finite())
                .then_some(constraints.max.width);
            constraints.constrain(cx.text().measure(text, max_width, options))
        });
        ui.paint(move |cx, area| cx.context_mut().paint(area, &self));
    }
}

impl FrameText {
    /// placement used for text system queries
    #[inline]
    pub fn request(&self) -> TextRequest {
        TextRequest::new(self.text)
            .offset(self.offset)
            .options(self.options)
    }

    /// missing spans repeat the last color
    pub fn span_color(&self, list: &DisplayList, span: usize) -> Color {
        let colors = list.store().view(self.colors);
        colors
            .get(span)
            .or(colors.last())
            .copied()
            .unwrap_or(Color::TRANSPARENT)
    }
}

impl Compare for FrameText {
    #[inline]
    fn equal(a: &Self, left: &DisplayList, b: &Self, right: &DisplayList) -> bool {
        a.request() == b.request() && left.store().view(a.colors) == right.store().view(b.colors)
    }

    fn hash(value: &Self, list: &DisplayList, state: &mut impl Hasher) {
        value.request().hash(state);
        list.store().view(value.colors).hash(state);
    }
}

//
// requests and runs
//

/// cached text geometry
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextRun(pub u64);

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
    /// placed text for text system queries such as cursor and hit testing
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct TextRequest {
        new(text: TextRun),
        offset: LogicalPoint = LogicalPoint::new(0.0, 0.0),
        options: TextOptions = TextOptions::new(),
    }
}

impl Hash for TextRequest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let float_bits = |value: f32| if value == 0.0 { 0 } else { value.to_bits() };
        self.text.hash(state);
        float_bits(self.offset.x).hash(state);
        float_bits(self.offset.y).hash(state);
        self.options.wrap.hash(state);
        self.options.overflow.hash(state);
        self.options.horizontal_align.hash(state);
        self.options.vertical_align.hash(state);
        self.options.max_lines.hash(state);
    }
}

//
// style and fonts
//

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
    /// style overrides for a rich text span
    #[derive(Clone, Copy, Debug, PartialEq)]
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
pub struct FontId(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct FontSettings<'a> {
        new(),
        features: &'a [([u8; 4], u32)] = &[],
        variations: &'a [([u8; 4], f32)] = &[],
        #[into]
        language: Option<&'a str> = None,
        optical_size: OpticalSize = OpticalSize::Auto,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum OpticalSize {
    #[default]
    Auto,
    FontDefault,
    Fixed(f32),
}

//
// options
//

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
