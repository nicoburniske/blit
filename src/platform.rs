use std::{marker::PhantomData, ptr::NonNull, rc::Rc};

use crate::{
    ImageData, ImageHandle, ImageId, KeyboardRequest, LogicalPoint, LogicalRect, LogicalSize,
    PhysicalRect, TextRequest,
    widgets::{BorderRadius, BoxShadowRequest, ImageRequest, Rectangle},
};

/// records a complete scene, then commits final damage in [`PlatformImpl::end_frame`]
pub trait PlatformImpl {
    fn begin_frame(&mut self);
    fn end_frame(&mut self, damage: &[PhysicalRect]);
    fn screen(&mut self) -> PhysicalRect;
    fn scale_factor(&mut self) -> f32 {
        1.0
    }
    fn push_rounded_clip(&mut self, area: LogicalRect, radius: BorderRadius);
    fn pop_rounded_clip(&mut self);
    fn draw_rectangle(&mut self, rectangle: &Rectangle<'_>, clip: PhysicalRect);
    fn draw_box_shadow(&mut self, shadow: &BoxShadowRequest, clip: PhysicalRect);
    fn create_image(&mut self, data: ImageData) -> ImageId;
    fn drop_image(&mut self, image: ImageId);
    fn draw_image(&mut self, image: &ImageRequest, clip: PhysicalRect);
    /// records text and returns its clipped physical ink bounds
    fn draw_text(&mut self, request: &TextRequest<'_>, clip: PhysicalRect) -> Option<PhysicalRect>;
    fn text_offset_at_position(
        &mut self,
        request: &TextRequest<'_>,
        position: LogicalPoint,
    ) -> usize;
    /// returns the typographic size after wrapping and overflow handling
    ///
    /// includes whitespace width, ignores alignment and offsets, and reports the full content size
    /// for clipped overflow
    fn measure_text(&mut self, request: &TextRequest<'_>) -> LogicalSize;
    fn text_cursor_rect(&mut self, request: &TextRequest<'_>, byte_offset: usize) -> LogicalRect;
    fn show_keyboard(&mut self, request: &KeyboardRequest<'_>);
}

pub struct Platform {
    inner: NonNull<dyn PlatformImpl>,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl Platform {
    #[inline]
    pub fn screen(&mut self) -> PhysicalRect {
        self.inner().screen()
    }

    #[inline]
    pub fn scale_factor(&mut self) -> f32 {
        self.inner().scale_factor()
    }

    #[inline]
    pub fn push_rounded_clip(&mut self, area: LogicalRect, radius: BorderRadius) {
        self.inner().push_rounded_clip(area, radius)
    }

    #[inline]
    pub fn pop_rounded_clip(&mut self) {
        self.inner().pop_rounded_clip()
    }

    #[inline]
    pub fn draw_rectangle(&mut self, rectangle: &Rectangle<'_>, clip: PhysicalRect) {
        self.inner().draw_rectangle(rectangle, clip)
    }

    #[inline]
    pub fn draw_box_shadow(&mut self, shadow: &BoxShadowRequest, clip: PhysicalRect) {
        self.inner().draw_box_shadow(shadow, clip)
    }

    #[inline]
    pub fn create_image(&mut self, image: ImageData) -> ImageHandle {
        let size = image.size;
        let id = self.inner().create_image(image);
        ImageHandle::new(id, size, self.inner)
    }

    #[inline]
    pub fn draw_image(&mut self, image: &ImageRequest, clip: PhysicalRect) {
        self.inner().draw_image(image, clip)
    }

    #[inline]
    pub fn draw_text(
        &mut self,
        request: &TextRequest<'_>,
        clip: PhysicalRect,
    ) -> Option<PhysicalRect> {
        self.inner().draw_text(request, clip)
    }

    #[inline]
    pub fn text_offset_at_position(
        &mut self,
        request: &TextRequest<'_>,
        position: LogicalPoint,
    ) -> usize {
        self.inner().text_offset_at_position(request, position)
    }

    #[inline]
    pub fn measure_text(&mut self, request: &TextRequest<'_>) -> LogicalSize {
        self.inner().measure_text(request)
    }

    #[inline]
    pub fn text_cursor_rect(
        &mut self,
        request: &TextRequest<'_>,
        byte_offset: usize,
    ) -> LogicalRect {
        self.inner().text_cursor_rect(request, byte_offset)
    }

    #[inline]
    pub fn show_keyboard(&mut self, request: &KeyboardRequest<'_>) {
        self.inner().show_keyboard(request)
    }
}

impl Platform {
    pub(crate) fn new<T: PlatformImpl + 'static>(implementation: &mut T) -> Self {
        Self {
            inner: NonNull::from(implementation as &mut dyn PlatformImpl),
            not_send_or_sync: PhantomData,
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.inner().begin_frame()
    }

    pub(crate) fn end_frame(&mut self, damage: &[PhysicalRect]) {
        self.inner().end_frame(damage)
    }

    #[inline]
    fn inner(&mut self) -> &mut dyn PlatformImpl {
        // safety: Runtime keeps the boxed implementation alive and stable
        unsafe { self.inner.as_mut() }
    }
}
