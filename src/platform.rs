use std::{marker::PhantomData, ptr::NonNull, rc::Rc};

use crate::{
    ImageData, ImageHandle, ImageId, KeyboardRequest, LogicalPoint, LogicalRect, PhysicalRect,
    TextRequest,
    widgets::{BorderRadius, BoxShadowRequest, ImageRequest, Rectangle},
};

pub trait PlatformImpl {
    fn begin_frame(&mut self, _: &[PhysicalRect]) {}
    fn add_damage(&mut self, area: PhysicalRect);
    fn end_frame(&mut self) {}
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
    // todo: prepare text once when tight measured damage bounds are needed
    fn draw_text(&mut self, request: &TextRequest<'_>, clip: PhysicalRect);
    fn text_offset_at_position(
        &mut self,
        request: &TextRequest<'_>,
        position: LogicalPoint,
    ) -> usize;
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
    pub fn draw_text(&mut self, request: &TextRequest<'_>, clip: PhysicalRect) {
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
    pub unsafe fn new<T: PlatformImpl + 'static>(implementation: &mut T) -> Self {
        Self {
            inner: NonNull::from(implementation as &mut dyn PlatformImpl),
            not_send_or_sync: PhantomData,
        }
    }

    pub(crate) fn begin_frame(&mut self, damage: &[PhysicalRect]) {
        self.inner().begin_frame(damage)
    }

    pub(crate) fn add_damage(&mut self, area: PhysicalRect) {
        self.inner().add_damage(area)
    }

    pub(crate) fn end_frame(&mut self) {
        self.inner().end_frame()
    }

    #[inline]
    fn inner(&mut self) -> &mut dyn PlatformImpl {
        // safety: platform is strictly single threaded
        // and never returns borrowed data from implementation
        unsafe { self.inner.as_mut() }
    }
}
