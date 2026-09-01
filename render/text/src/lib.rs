mod cosmic;

pub use cosmic::CosmicBackend;

use std::{marker::PhantomData, ptr::NonNull, sync::Arc};

use blit::{LogicalPoint, LogicalRect, LogicalSize};

pub trait Backend: 'static {
    fn system_font(&mut self, request: SystemFontRequest<'_>) -> Result<FontId, FontError>;

    fn register_font(&mut self, data: FontData, face_index: u32) -> Result<FontId, FontError>;

    fn font(&self, font: FontId) -> Option<FontFace>;

    fn text(&mut self, text: &str, style: TextStyle) -> TextId;

    fn layout(&mut self, request: TextLayoutRequest) -> TextLayoutId;

    fn size(&self, layout: TextLayoutId) -> LogicalSize;

    fn hit_test(&self, layout: TextLayoutId, position: LogicalPoint) -> usize;

    fn cursor_rect(&self, layout: TextLayoutId, byte_offset: usize) -> LogicalRect;

    fn visit_runs(&self, layout: TextLayoutId, visitor: &mut GlyphRunVisitor<'_>);

    fn finish_frame(&mut self) {}
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

impl FontData {
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Static(data) => data,
            Self::Shared(data) => data,
        }
    }
}

impl AsRef<[u8]> for FontData {
    fn as_ref(&self) -> &[u8] {
        self.bytes()
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLayoutRequest {
    pub text: TextId,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub offset_x: f32,
    pub max_lines: Option<u16>,
    pub wrap: TextWrap,
    pub overflow: TextOverflow,
    pub horizontal_align: HorizontalAlign,
    pub vertical_align: VerticalAlign,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextLayoutId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphRun<'a> {
    pub font: FontId,
    pub size: f32,
    pub bounds: LogicalRect,
    pub glyphs: &'a [Glyph],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u32,
    /// pen origin in logical coordinates relative to the layout origin
    pub position: LogicalPoint,
    pub advance: f32,
    pub cluster: u32,
}

pub struct GlyphRunVisitor<'a> {
    data: NonNull<()>,
    visit: unsafe fn(NonNull<()>, GlyphRun<'_>),
    marker: PhantomData<&'a mut ()>,
}

impl GlyphRunVisitor<'_> {
    pub fn push(&mut self, run: GlyphRun<'_>) {
        dispatch::push(self, run)
    }
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
    text: fn(NonNull<()>, &str, TextStyle) -> TextId,
    layout: fn(NonNull<()>, TextLayoutRequest) -> TextLayoutId,
    size: fn(NonNull<()>, TextLayoutId) -> LogicalSize,
    hit_test: fn(NonNull<()>, TextLayoutId, LogicalPoint) -> usize,
    cursor_rect: fn(NonNull<()>, TextLayoutId, usize) -> LogicalRect,
    visit_runs: fn(NonNull<()>, TextLayoutId, &mut GlyphRunVisitor<'_>),
    finish_frame: fn(NonNull<()>),
}

impl TextSystem {
    pub fn new<B: Backend>(backend: B) -> Self {
        Self {
            data: NonNull::from(Box::leak(Box::new(backend))).cast(),
            drop: dispatch::drop_backend::<B>,
            system_font: dispatch::system_font::<B>,
            register_font: dispatch::register_font::<B>,
            font: dispatch::font::<B>,
            text: dispatch::text::<B>,
            layout: dispatch::layout::<B>,
            size: dispatch::size::<B>,
            hit_test: dispatch::hit_test::<B>,
            cursor_rect: dispatch::cursor_rect::<B>,
            visit_runs: dispatch::visit_runs::<B>,
            finish_frame: dispatch::finish_frame::<B>,
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

    pub fn text(&mut self, text: &str, style: TextStyle) -> TextId {
        (self.text)(self.data, text, style)
    }

    pub fn layout(&mut self, request: TextLayoutRequest) -> TextLayoutId {
        (self.layout)(self.data, request)
    }

    pub fn size(&self, layout: TextLayoutId) -> LogicalSize {
        (self.size)(self.data, layout)
    }

    pub fn hit_test(&self, layout: TextLayoutId, position: LogicalPoint) -> usize {
        (self.hit_test)(self.data, layout, position)
    }

    pub fn cursor_rect(&self, layout: TextLayoutId, byte_offset: usize) -> LogicalRect {
        (self.cursor_rect)(self.data, layout, byte_offset)
    }

    pub fn visit_runs<F>(&self, layout: TextLayoutId, mut visit: F)
    where
        F: for<'a> FnMut(GlyphRun<'a>),
    {
        let mut visitor = dispatch::visitor(&mut visit);
        (self.visit_runs)(self.data, layout, &mut visitor)
    }

    pub fn finish_frame(&mut self) {
        (self.finish_frame)(self.data)
    }
}

impl Drop for TextSystem {
    fn drop(&mut self) {
        (self.drop)(self.data)
    }
}

mod dispatch {
    //!
    //!  safety: TextSystem owns data as Box<B> and dispatches according to its borrow
    //!
    use super::*;

    pub fn drop_backend<B: Backend>(data: NonNull<()>) {
        unsafe { drop(Box::from_raw(data.cast::<B>().as_ptr())) }
    }

    fn backend<B: Backend>(data: NonNull<()>) -> &'static B {
        unsafe { data.cast::<B>().as_ref() }
    }

    fn backend_mut<B: Backend>(data: NonNull<()>) -> &'static mut B {
        unsafe { data.cast::<B>().as_mut() }
    }

    pub fn visitor<'a, F>(visit: &'a mut F) -> GlyphRunVisitor<'a>
    where
        F: for<'run> FnMut(GlyphRun<'run>),
    {
        unsafe fn call<F>(data: NonNull<()>, run: GlyphRun<'_>)
        where
            F: for<'run> FnMut(GlyphRun<'run>),
        {
            (unsafe { data.cast::<F>().as_mut() })(run)
        }

        GlyphRunVisitor {
            data: NonNull::from(visit).cast(),
            visit: call::<F>,
            marker: PhantomData,
        }
    }

    pub fn push(visitor: &mut GlyphRunVisitor<'_>, run: GlyphRun<'_>) {
        unsafe { (visitor.visit)(visitor.data, run) }
    }

    pub fn system_font<B: Backend>(
        data: NonNull<()>,
        request: SystemFontRequest<'_>,
    ) -> Result<FontId, FontError> {
        backend_mut::<B>(data).system_font(request)
    }

    pub fn register_font<B: Backend>(
        data: NonNull<()>,
        font: FontData,
        face_index: u32,
    ) -> Result<FontId, FontError> {
        backend_mut::<B>(data).register_font(font, face_index)
    }

    pub fn font<B: Backend>(data: NonNull<()>, font: FontId) -> Option<FontFace> {
        backend::<B>(data).font(font)
    }

    pub fn text<B: Backend>(data: NonNull<()>, text: &str, style: TextStyle) -> TextId {
        backend_mut::<B>(data).text(text, style)
    }

    pub fn layout<B: Backend>(data: NonNull<()>, request: TextLayoutRequest) -> TextLayoutId {
        backend_mut::<B>(data).layout(request)
    }

    pub fn size<B: Backend>(data: NonNull<()>, layout: TextLayoutId) -> LogicalSize {
        backend::<B>(data).size(layout)
    }

    pub fn hit_test<B: Backend>(
        data: NonNull<()>,
        layout: TextLayoutId,
        position: LogicalPoint,
    ) -> usize {
        backend::<B>(data).hit_test(layout, position)
    }

    pub fn cursor_rect<B: Backend>(
        data: NonNull<()>,
        layout: TextLayoutId,
        byte_offset: usize,
    ) -> LogicalRect {
        backend::<B>(data).cursor_rect(layout, byte_offset)
    }

    pub fn visit_runs<B: Backend>(
        data: NonNull<()>,
        layout: TextLayoutId,
        visitor: &mut GlyphRunVisitor<'_>,
    ) {
        backend::<B>(data).visit_runs(layout, visitor)
    }

    pub fn finish_frame<B: Backend>(data: NonNull<()>) {
        backend_mut::<B>(data).finish_frame()
    }
}
