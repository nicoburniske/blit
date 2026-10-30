use std::{hash::Hash, ops::Range, sync::Arc};

use blit::{LogicalPoint, LogicalRect, LogicalSize, PhysicalPoint, Point};

pub trait TextLayoutEngine: 'static {
    type Shape: 'static;
    type GlyphKey: Eq + Hash + 'static;

    fn register_stack(&mut self, families: &[Family]) -> Result<FontStackId, FontError>;

    fn prepare_glyph(&mut self, shape: &Self::Shape, request: GlyphRequest) -> PreparedGlyph<Self::GlyphKey>;

    /// equal keys must produce identical pixels and bearings
    fn rasterize_glyph(&mut self, key: &Self::GlyphKey) -> Result<GlyphImage, FontError>;

    fn shape(&mut self, text: Text<'_>) -> (Self::Shape, usize);

    /// font indices remain valid until the shape is dropped
    fn layout(&mut self, shape: &mut Self::Shape, text: &str, request: LayoutRequest) -> TextLayout;

    fn carets(&mut self, shape: &mut Self::Shape, text: &str, request: LayoutRequest, line: usize) -> Box<[Caret]>;
}

#[derive(Clone, Copy, Debug)]
pub struct Text<'a> {
    pub text: &'a str,
    pub spans: &'a [TextSpan<'a>],
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextSpan<'a> {
    pub range: Range<usize>,
    pub style: TextStyle<'a>,
}

impl<'a> Text<'a> {
    pub fn segments(&self) -> impl Iterator<Item = (usize, Range<usize>, TextStyle<'a>)> + '_ {
        assert!(!self.spans.is_empty());
        let mut end = 0;
        for span in self.spans {
            assert_eq!(span.range.start, end);
            assert!(self.text.is_char_boundary(span.range.end));
            end = span.range.end;
        }
        assert_eq!(end, self.text.len());
        self.spans
            .iter()
            .enumerate()
            .map(|(index, span)| (index, span.range.clone(), span.style))
    }
}

#[derive(Clone, Debug)]
pub enum FontData {
    Static(&'static [u8]),
    Shared(Arc<[u8]>),
}

#[derive(Clone, Debug)]
pub enum Family {
    Embedded(Vec<FontData>),
    Named(String),
}

impl Family {
    pub fn embedded(fonts: impl IntoIterator<Item = FontData>) -> Self {
        Self::Embedded(fonts.into_iter().collect())
    }

    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }
}

pub struct GlyphRequest {
    pub font: usize,
    pub glyph: u16,
    pub scale: f32,
    /// glyph origin in device pixels
    pub origin: Point<f32>,
}

pub struct PreparedGlyph<K> {
    pub key: K,
    pub origin: PhysicalPoint,
}

#[derive(Debug, Default)]
pub struct GlyphImage {
    pub metrics: GlyphMetrics,
    pub pixels: GlyphPixels,
}

#[derive(Debug)]
pub enum GlyphPixels {
    /// tightly packed coverage tinted by the text color
    Alpha8(Box<[u8]>),
    /// tightly packed rgba with encoded srgb channels multiplied by alpha
    /// keeps intrinsic colors and receives text alpha once
    Rgba8(Box<[u8]>),
}

impl Default for GlyphPixels {
    fn default() -> Self {
        Self::Alpha8(Box::default())
    }
}

impl GlyphPixels {
    pub fn data(&self) -> &[u8] {
        match self {
            Self::Alpha8(data) | Self::Rgba8(data) => data,
        }
    }

    pub fn is_color(&self) -> bool {
        matches!(self, Self::Rgba8(_))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphMetrics {
    pub width: u16,
    pub height: u16,
    /// pixel offset from the glyph origin including the requested horizontal phase
    pub left: i16,
    /// pixel offset from the baseline with positive values pointing down
    pub top: i16,
}

impl AsRef<[u8]> for FontData {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Static(data) => data,
            Self::Shared(data) => data,
        }
    }
}

/// backend owned font stack
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontStackId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle<'a> {
    pub font: FontStackId,
    pub size: f32,
    pub weight: u16,
    pub stretch: u16,
    pub style: FontStyle,
    pub settings: FontSettings<'a>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FontSettings<'a> {
    pub features: &'a [([u8; 4], u32)],
    pub variations: &'a [([u8; 4], f32)],
    pub language: Option<&'a str>,
    pub optical_size: OpticalSize,
}

impl FontSettings<'_> {
    pub const fn new() -> Self {
        Self {
            features: &[],
            variations: &[],
            language: None,
            optical_size: OpticalSize::Auto,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum OpticalSize {
    #[default]
    Auto,
    FontDefault,
    Fixed(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutRequest {
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub max_lines: Option<u16>,
    pub wrap: TextWrap,
    pub overflow: TextOverflow,
}

#[derive(Debug, Default, PartialEq)]
pub struct TextLayout {
    pub size: LogicalSize,
    pub glyphs: Box<[Glyph]>,
    pub runs: Box<[LayoutRun]>,
    pub lines: Box<[LayoutLine]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutRun {
    /// backend font index within the shape
    pub font: usize,
    pub glyphs: Range<u32>,
    pub span: usize,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutLine {
    pub bounds: LogicalRect,
    pub text: Range<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caret {
    pub byte_offset: u32,
    pub position: LogicalPoint,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u16,
    /// pen origin in logical coordinates relative to the layout origin
    pub position: LogicalPoint,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontError {
    NotFound,
    InvalidData,
    Unsupported,
}

impl std::fmt::Display for FontError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => formatter.write_str("font not found"),
            Self::InvalidData => formatter.write_str("invalid font data"),
            Self::Unsupported => formatter.write_str("unsupported font"),
        }
    }
}

impl std::error::Error for FontError {}
