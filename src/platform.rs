use std::{marker::PhantomData, ptr::NonNull, rc::Rc};

use crate::{
    ImageData, ImageHandle, ImageId, KeyboardRequest, LogicalPoint, LogicalRect, PhysicalRect,
    TextRequest,
    widgets::{ImageRequest, Rectangle},
};

pub trait PlatformImpl {
    fn begin_frame(&mut self, _: &[PhysicalRect]) {}
    fn end_frame(&mut self) {}
    fn screen(&mut self) -> PhysicalRect;
    fn scale_factor(&mut self) -> f32 {
        1.0
    }
    fn draw_rectangle(&mut self, rectangle: &Rectangle, clip: PhysicalRect);
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
    implementation: NonNull<dyn PlatformImpl>,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl Platform {
    #[inline]
    pub fn screen(&mut self) -> PhysicalRect {
        unsafe { self.implementation.as_mut().screen() }
    }

    #[inline]
    pub fn scale_factor(&mut self) -> f32 {
        unsafe { self.implementation.as_mut().scale_factor() }
    }

    #[inline]
    pub fn draw_rectangle(&mut self, rectangle: &Rectangle, clip: PhysicalRect) {
        unsafe { self.implementation.as_mut().draw_rectangle(rectangle, clip) }
    }

    #[inline]
    pub fn create_image(&mut self, image: ImageData) -> ImageHandle {
        let size = image.size;
        let id = unsafe { self.implementation.as_mut().create_image(image) };
        ImageHandle::new(id, size, self.implementation)
    }

    #[inline]
    pub fn draw_image(&mut self, image: &ImageRequest, clip: PhysicalRect) {
        unsafe { self.implementation.as_mut().draw_image(image, clip) }
    }

    #[inline]
    pub fn draw_text(&mut self, request: &TextRequest<'_>, clip: PhysicalRect) {
        unsafe { self.implementation.as_mut().draw_text(request, clip) }
    }

    #[inline]
    pub fn text_offset_at_position(
        &mut self,
        request: &TextRequest<'_>,
        position: LogicalPoint,
    ) -> usize {
        unsafe {
            self.implementation
                .as_mut()
                .text_offset_at_position(request, position)
        }
    }

    #[inline]
    pub fn text_cursor_rect(
        &mut self,
        request: &TextRequest<'_>,
        byte_offset: usize,
    ) -> LogicalRect {
        unsafe {
            self.implementation
                .as_mut()
                .text_cursor_rect(request, byte_offset)
        }
    }

    #[inline]
    pub fn show_keyboard(&mut self, request: &KeyboardRequest<'_>) {
        unsafe { self.implementation.as_mut().show_keyboard(request) }
    }
}

impl Platform {
    pub unsafe fn new<T: PlatformImpl + 'static>(implementation: &mut T) -> Self {
        Self {
            implementation: NonNull::from(implementation as &mut dyn PlatformImpl),
            not_send_or_sync: PhantomData,
        }
    }

    pub(crate) fn begin_frame(&mut self, damage: &[PhysicalRect]) {
        unsafe { self.implementation.as_mut().begin_frame(damage) }
    }

    pub(crate) fn end_frame(&mut self) {
        unsafe { self.implementation.as_mut().end_frame() }
    }
}
