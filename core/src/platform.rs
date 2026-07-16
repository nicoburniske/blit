use std::{marker::PhantomData, ptr::NonNull, rc::Rc};

use crate::{
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    keyboard::KeyboardRequest,
    paint::TextRequest,
    paint_list::PaintList,
    resource::{ImageData, ImageHandle, ImageId, StringData, StringHandle, StringId},
    RepaintBuffer,
};

pub trait PlatformImpl {
    /// damage may overlap and each covered pixel must be rendered once
    fn render(&mut self, paint: &PaintList, damage: &[PhysicalRect]);
    fn screen(&mut self) -> PhysicalRect;
    fn scale_factor(&mut self) -> f32 { 1.0 }
    fn repaint_buffer(&self) -> RepaintBuffer { RepaintBuffer::Reused }

    fn create_image(&mut self, data: ImageData) -> ImageId;
    fn drop_image(&mut self, image: ImageId);

    fn create_string(&mut self, string: StringData) -> StringId;
    /// queues destruction after the current frame
    fn drop_string(&mut self, string: StringId);
    fn string(&self, string: StringId) -> &str;

    fn text_offset_at_position(&mut self, request: &TextRequest, position: LogicalPoint) -> usize;
    /// returns the typographic size after wrapping and overflow handling
    ///
    /// includes whitespace width, ignores alignment and offsets, and reports the full content size
    /// for clipped overflow
    fn measure_text(&mut self, request: &TextRequest) -> LogicalSize;
    /// returns the typographic height after wrapping
    fn measure_text_height(&mut self, request: &TextRequest) -> f32;
    /// returns the cursor position and line height for the nearest valid byte offset
    fn text_cursor_rect(&mut self, request: &TextRequest, byte_offset: usize) -> LogicalRect;

    fn show_keyboard(&mut self, request: &KeyboardRequest<'_>);
}

#[derive(Clone, Copy, Debug)]
pub struct Platform {
    inner: NonNull<dyn PlatformImpl>,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl Platform {
    #[inline]
    pub fn screen(&mut self) -> PhysicalRect { self.inner().screen() }

    #[inline]
    pub fn scale_factor(&mut self) -> f32 { self.inner().scale_factor() }

    #[inline]
    pub fn create_image(&mut self, image: ImageData) -> ImageHandle {
        let size = image.size;
        let id = self.inner().create_image(image);
        ImageHandle::new(id, size, self.inner)
    }

    pub fn create_string(&mut self, string: impl Into<StringData>) -> StringHandle {
        let id = self.inner().create_string(string.into());
        StringHandle::new(id, *self)
    }

    #[inline]
    pub fn text_offset_at_position(&mut self, request: &TextRequest, position: LogicalPoint) -> usize {
        self.inner().text_offset_at_position(request, position)
    }

    #[inline]
    pub fn measure_text(&mut self, request: &TextRequest) -> LogicalSize {
        self.inner().measure_text(request)
    }

    #[inline]
    pub fn measure_text_height(&mut self, request: &TextRequest) -> f32 {
        self.inner().measure_text_height(request)
    }

    #[inline]
    pub fn text_cursor_rect(&mut self, request: &TextRequest, byte_offset: usize) -> LogicalRect {
        self.inner().text_cursor_rect(request, byte_offset)
    }

    #[inline]
    pub fn show_keyboard(&mut self, request: &KeyboardRequest<'_>) { self.inner().show_keyboard(request) }
}

impl Platform {
    pub(crate) fn new<T: PlatformImpl + 'static>(implementation: &mut T) -> Self {
        Self { inner: NonNull::from(implementation as &mut dyn PlatformImpl), not_send_or_sync: PhantomData }
    }

    #[inline]
    pub(crate) fn inner(&self) -> &mut dyn PlatformImpl {
        // safety: Runtime keeps the boxed implementation alive and stable
        unsafe { &mut *self.inner.as_ptr() }
    }
}
