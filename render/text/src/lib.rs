#[cfg(feature = "cosmic")]
pub mod cosmic;

use std::{mem::size_of, ops::Range, ptr::NonNull, sync::Arc};

use blit::{LogicalPoint, LogicalRect, LogicalSize};

pub trait TextBackend: 'static {
    fn system_font(&mut self, request: SystemFontRequest<'_>) -> Result<FontId, FontError>;

    fn register_font(&mut self, data: FontData, face_index: u32) -> Result<FontId, FontError>;

    fn font(&self, font: FontId) -> Option<FontFace>;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontId(pub u64);

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
    pub font: FontId,
    pub size: f32,
    pub weight: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutRequest {
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub offset_x: f32,
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

impl TextLayout {
    pub fn allocated_bytes(&self) -> usize {
        self.glyphs.len() * size_of::<Glyph>()
            + self.runs.len() * size_of::<LayoutRun>()
            + self.lines.len() * size_of::<LayoutLine>()
            + self.carets.len() * size_of::<Caret>()
    }

    pub fn hit_test(&self, position: LogicalPoint) -> usize {
        let Some(line) = self.lines.iter().min_by(|left, right| {
            let left_distance = if position.y < left.bounds.y {
                left.bounds.y - position.y
            } else if position.y > left.bounds.y + left.bounds.height {
                position.y - left.bounds.y - left.bounds.height
            } else {
                0.0
            };
            let right_distance = if position.y < right.bounds.y {
                right.bounds.y - position.y
            } else if position.y > right.bounds.y + right.bounds.height {
                position.y - right.bounds.y - right.bounds.height
            } else {
                0.0
            };
            left_distance.total_cmp(&right_distance)
        }) else {
            return 0;
        };
        self.carets[line.carets.start as usize..line.carets.end as usize]
            .iter()
            .min_by(|left, right| {
                (left.position.x - position.x)
                    .abs()
                    .total_cmp(&(right.position.x - position.x).abs())
            })
            .map_or(0, |caret| caret.byte_offset as usize)
    }

    pub fn cursor_rect(&self, byte_offset: usize) -> LogicalRect {
        let Some(caret) = self
            .carets
            .iter()
            .min_by_key(|caret| (caret.byte_offset as usize).abs_diff(byte_offset))
        else {
            return LogicalRect::default();
        };
        LogicalRect {
            x: caret.position.x,
            y: caret.position.y,
            width: 0.0,
            height: caret.height,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutRun {
    pub font: FontId,
    pub size: f32,
    pub bounds: LogicalRect,
    pub glyphs: Range<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutLine {
    pub bounds: LogicalRect,
    pub runs: Range<u32>,
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
    pub id: u32,
    /// pen origin in logical coordinates relative to the layout origin
    pub position: LogicalPoint,
    pub advance: f32,
    pub cluster: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextWrap {
    #[default]
    None,
    Word,
    Glyph,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HorizontalAlign {
    #[default]
    Start,
    Center,
    End,
    Justify,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VerticalAlign {
    #[default]
    Start,
    Center,
    End,
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

pub struct TextSystem {
    data: NonNull<()>,
    drop: fn(NonNull<()>),
    system_font: fn(NonNull<()>, SystemFontRequest<'_>) -> Result<FontId, FontError>,
    register_font: fn(NonNull<()>, FontData, u32) -> Result<FontId, FontError>,
    font: fn(NonNull<()>, FontId) -> Option<FontFace>,
    layout: fn(NonNull<()>, &str, TextStyle, LayoutRequest) -> TextLayout,
}

impl TextSystem {
    pub fn new<B: TextBackend>(backend: B) -> Self {
        Self {
            data: NonNull::from(Box::leak(Box::new(backend))).cast(),
            drop: dispatch::drop_backend::<B>,
            system_font: dispatch::system_font::<B>,
            register_font: dispatch::register_font::<B>,
            font: dispatch::font::<B>,
            layout: dispatch::layout::<B>,
        }
    }

    pub fn system_font(&mut self, request: SystemFontRequest<'_>) -> Result<FontId, FontError> {
        (self.system_font)(self.data, request)
    }

    pub fn register_font(&mut self, data: FontData, face_index: u32) -> Result<FontId, FontError> {
        (self.register_font)(self.data, data, face_index)
    }

    pub fn font(&self, font: FontId) -> Option<FontFace> {
        (self.font)(self.data, font)
    }

    pub fn layout(&mut self, text: &str, style: TextStyle, request: LayoutRequest) -> TextLayout {
        (self.layout)(self.data, text, style, request)
    }
}

impl Drop for TextSystem {
    fn drop(&mut self) {
        (self.drop)(self.data)
    }
}

mod dispatch {
    // safety: TextSystem owns data as Box<B> and dispatches according to its borrow
    use super::*;

    pub fn drop_backend<B: TextBackend>(data: NonNull<()>) {
        unsafe { drop(Box::from_raw(data.cast::<B>().as_ptr())) }
    }

    fn backend<B: TextBackend>(data: NonNull<()>) -> &'static B {
        unsafe { data.cast::<B>().as_ref() }
    }

    fn backend_mut<B: TextBackend>(data: NonNull<()>) -> &'static mut B {
        unsafe { data.cast::<B>().as_mut() }
    }

    pub fn system_font<B: TextBackend>(
        data: NonNull<()>,
        request: SystemFontRequest<'_>,
    ) -> Result<FontId, FontError> {
        backend_mut::<B>(data).system_font(request)
    }

    pub fn register_font<B: TextBackend>(
        data: NonNull<()>,
        font: FontData,
        face_index: u32,
    ) -> Result<FontId, FontError> {
        backend_mut::<B>(data).register_font(font, face_index)
    }

    pub fn font<B: TextBackend>(data: NonNull<()>, font: FontId) -> Option<FontFace> {
        backend::<B>(data).font(font)
    }

    pub fn layout<B: TextBackend>(
        data: NonNull<()>,
        text: &str,
        style: TextStyle,
        request: LayoutRequest,
    ) -> TextLayout {
        backend_mut::<B>(data).layout(text, style, request)
    }
}
