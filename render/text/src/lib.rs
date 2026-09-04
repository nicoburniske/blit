use std::{ops::Range, sync::Arc};

use blit::{LogicalPoint, LogicalRect, LogicalSize};

/// todo: support variable font instances and expose their coordinates to renderers
pub trait TextLayoutEngine: 'static {
    fn system_font(&mut self, _request: SystemFontRequest<'_>) -> Result<FontFaceId, FontError> {
        Err(FontError::Unsupported)
    }

    fn register_font(&mut self, data: FontData, face_index: u32) -> Result<FontFaceId, FontError>;

    fn register_font_selection(
        &mut self,
        candidates: &[FontCandidate],
    ) -> Result<FontSelectionId, FontError>;

    fn font_face(&self, face: FontFaceId) -> Option<&FontFace>;

    fn layout(&mut self, text: &str, style: TextStyle, request: LayoutRequest) -> TextLayout;
}

#[derive(Clone, Debug)]
pub enum FontData {
    Static(&'static [u8]),
    Shared(Arc<[u8]>),
}

#[derive(Clone, Debug)]
pub struct FontFace {
    pub data: FontData,
    pub face_index: u32,
}

impl AsRef<[u8]> for FontData {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Static(data) => data,
            Self::Shared(data) => data,
        }
    }
}

/// backend owned font selection
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontSelectionId(pub u64);

/// exact face used by glyph ids
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontFaceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontCandidate {
    pub face: FontFaceId,
    pub weight: u16,
    pub stretch: u16,
    pub style: FontStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemFontRequest<'a> {
    pub family: &'a str,
    pub weight: u16,
    pub stretch: u16,
    pub style: FontStyle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub font: FontSelectionId,
    pub size: f32,
    pub weight: u16,
    pub stretch: u16,
    pub style: FontStyle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutRequest {
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub max_lines: Option<u16>,
    pub wrap: TextWrap,
    pub overflow: TextOverflow,
    pub horizontal_align: HorizontalAlign,
    pub vertical_align: VerticalAlign,
}

#[derive(Debug, PartialEq)]
pub struct TextLayout {
    pub size: LogicalSize,
    pub glyphs: Box<[Glyph]>,
    pub runs: Box<[LayoutRun]>,
    pub lines: Box<[LayoutLine]>,
    pub carets: Box<[Caret]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutRun {
    pub face: FontFaceId,
    pub size: f32,
    pub glyphs: Range<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutLine {
    pub bounds: LogicalRect,
    pub carets: Range<u32>,
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
