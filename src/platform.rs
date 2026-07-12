use std::{marker::PhantomData, ptr::NonNull, rc::Rc};

use crate::{
    ImageData, ImageId, ImageResource, KeyboardRequest, LogicalPoint, LogicalRect, PhysicalRect,
    TextRequest,
    widgets::{ImageRequest, Rectangle},
};

pub trait PlatformImpl {
    fn begin_frame(&mut self) {}
    fn end_frame(&mut self) {}
    fn screen(&mut self) -> PhysicalRect;
    fn scale_factor(&mut self) -> f32 {
        1.0
    }
    fn draw_rectangle(&mut self, rectangle: &Rectangle, clips: &[PhysicalRect]);
    fn create_image(&mut self, data: ImageData) -> ImageId;
    fn drop_image(&mut self, image: ImageId);
    fn draw_image(&mut self, image: &ImageRequest, clips: &[PhysicalRect]);
    // todo: prepare text once when tight measured damage bounds are needed
    fn draw_text(&mut self, request: &TextRequest<'_>, clips: &[PhysicalRect]);
    fn text_offset_at_position(
        &mut self,
        request: &TextRequest<'_>,
        position: LogicalPoint,
    ) -> usize;
    fn text_cursor_rect(&mut self, request: &TextRequest<'_>, byte_offset: usize) -> LogicalRect;
    fn show_keyboard(&mut self, request: &KeyboardRequest<'_>);
}

pub struct Platform {
    data: NonNull<()>,
    vtable: &'static PlatformVTable,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl Platform {
    #[inline]
    pub fn screen(&mut self) -> PhysicalRect {
        unsafe { (self.vtable.screen)(self.data) }
    }

    #[inline]
    pub fn scale_factor(&mut self) -> f32 {
        unsafe { (self.vtable.scale_factor)(self.data) }
    }

    #[inline]
    pub fn draw_rectangle(&mut self, rectangle: &Rectangle, clips: &[PhysicalRect]) {
        unsafe { (self.vtable.draw_rectangle)(self.data, rectangle, clips) }
    }

    #[inline]
    pub fn create_image(&mut self, image: ImageData) -> ImageResource {
        let size = image.size;
        let id = unsafe { (self.vtable.create_image)(self.data, image) };
        ImageResource::new(id, size, self.data, self.vtable.drop_image)
    }

    #[inline]
    pub fn draw_image(&mut self, image: &ImageRequest, clips: &[PhysicalRect]) {
        unsafe { (self.vtable.draw_image)(self.data, image, clips) }
    }

    #[inline]
    pub fn draw_text(&mut self, request: &TextRequest<'_>, clips: &[PhysicalRect]) {
        unsafe { (self.vtable.draw_text)(self.data, request, clips) }
    }

    #[inline]
    pub fn text_offset_at_position(
        &mut self,
        request: &TextRequest<'_>,
        position: LogicalPoint,
    ) -> usize {
        unsafe { (self.vtable.text_offset_at_position)(self.data, request, position) }
    }

    #[inline]
    pub fn text_cursor_rect(
        &mut self,
        request: &TextRequest<'_>,
        byte_offset: usize,
    ) -> LogicalRect {
        unsafe { (self.vtable.text_cursor_rect)(self.data, request, byte_offset) }
    }

    #[inline]
    pub fn show_keyboard(&mut self, request: &KeyboardRequest<'_>) {
        unsafe { (self.vtable.show_keyboard)(self.data, request) }
    }
}

impl Platform {
    pub unsafe fn new<T: PlatformImpl>(implementation: &mut T) -> Self {
        Self {
            data: NonNull::from(implementation).cast(),
            vtable: vtable::<T>(),
            not_send_or_sync: PhantomData,
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        unsafe { (self.vtable.begin_frame)(self.data) }
    }

    pub(crate) fn end_frame(&mut self) {
        unsafe { (self.vtable.end_frame)(self.data) }
    }
}

pub struct PlatformVTable {
    begin_frame: unsafe fn(NonNull<()>),
    end_frame: unsafe fn(NonNull<()>),
    screen: unsafe fn(NonNull<()>) -> PhysicalRect,
    scale_factor: unsafe fn(NonNull<()>) -> f32,
    draw_rectangle: unsafe fn(NonNull<()>, &Rectangle, &[PhysicalRect]),
    create_image: unsafe fn(NonNull<()>, ImageData) -> ImageId,
    drop_image: unsafe fn(NonNull<()>, ImageId),
    draw_image: unsafe fn(NonNull<()>, &ImageRequest, &[PhysicalRect]),
    draw_text: unsafe fn(NonNull<()>, &TextRequest<'_>, &[PhysicalRect]),
    text_offset_at_position: unsafe fn(NonNull<()>, &TextRequest<'_>, LogicalPoint) -> usize,
    text_cursor_rect: unsafe fn(NonNull<()>, &TextRequest<'_>, usize) -> LogicalRect,
    show_keyboard: unsafe fn(NonNull<()>, &KeyboardRequest<'_>),
}

fn vtable<T: PlatformImpl>() -> &'static PlatformVTable {
    &PlatformVTable {
        begin_frame: |data| unsafe { data.cast::<T>().as_mut() }.begin_frame(),
        end_frame: |data| unsafe { data.cast::<T>().as_mut() }.end_frame(),
        screen: |data| unsafe { data.cast::<T>().as_mut() }.screen(),
        scale_factor: |data| unsafe { data.cast::<T>().as_mut() }.scale_factor(),
        draw_rectangle: |data, request, clips| {
            unsafe { data.cast::<T>().as_mut() }.draw_rectangle(request, clips)
        },
        create_image: |data, image| unsafe { data.cast::<T>().as_mut() }.create_image(image),
        drop_image: |data, image| unsafe { data.cast::<T>().as_mut() }.drop_image(image),
        draw_image: |data, image, clips| {
            unsafe { data.cast::<T>().as_mut() }.draw_image(image, clips)
        },
        draw_text: |data, request, clips| {
            unsafe { data.cast::<T>().as_mut() }.draw_text(request, clips)
        },
        text_offset_at_position: |data, request, position| {
            unsafe { data.cast::<T>().as_mut() }.text_offset_at_position(request, position)
        },
        text_cursor_rect: |data, request, byte_offset| {
            unsafe { data.cast::<T>().as_mut() }.text_cursor_rect(request, byte_offset)
        },
        show_keyboard: |data, request| unsafe { data.cast::<T>().as_mut() }.show_keyboard(request),
    }
}
