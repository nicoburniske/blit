use std::{marker::PhantomData, ptr::NonNull, rc::Rc};

use crate::{
    PhysicalRect, TextRequest,
    widgets::{Image, Rectangle},
};

pub trait PlatformImpl {
    fn screen(&mut self) -> PhysicalRect;
    fn scale_factor(&mut self) -> f32 {
        1.0
    }
    fn draw_rectangle(&mut self, rectangle: &Rectangle, clips: &[PhysicalRect]);
    fn draw_image(&mut self, image: &Image<'_>, clips: &[PhysicalRect]);
    // todo: prepare text once when tight measured damage bounds are needed
    fn draw_text(&mut self, request: &TextRequest<'_>, clips: &[PhysicalRect]);
}

#[derive(Clone, Copy)]
pub struct Platform {
    data: NonNull<()>,
    vtable: &'static PlatformVTable,
    not_send_or_sync: PhantomData<Rc<()>>,
}

pub struct PlatformVTable {
    screen: unsafe fn(NonNull<()>) -> PhysicalRect,
    scale_factor: unsafe fn(NonNull<()>) -> f32,
    draw_rectangle: unsafe fn(NonNull<()>, &Rectangle, &[PhysicalRect]),
    draw_image: unsafe fn(NonNull<()>, &Image<'_>, &[PhysicalRect]),
    draw_text: unsafe fn(NonNull<()>, &TextRequest<'_>, &[PhysicalRect]),
}

fn vtable<T: PlatformImpl>() -> &'static PlatformVTable {
    &PlatformVTable {
        screen: |data| unsafe { data.cast::<T>().as_mut() }.screen(),
        scale_factor: |data| unsafe { data.cast::<T>().as_mut() }.scale_factor(),
        draw_rectangle: |data, request, clips| {
            unsafe { data.cast::<T>().as_mut() }.draw_rectangle(request, clips)
        },
        draw_image: |data, image, clips| {
            unsafe { data.cast::<T>().as_mut() }.draw_image(image, clips)
        },
        draw_text: |data, request, clips| {
            unsafe { data.cast::<T>().as_mut() }.draw_text(request, clips)
        },
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

    #[inline]
    pub fn screen(self) -> PhysicalRect {
        unsafe { (self.vtable.screen)(self.data) }
    }

    #[inline]
    pub fn scale_factor(self) -> f32 {
        unsafe { (self.vtable.scale_factor)(self.data) }
    }

    #[inline]
    pub fn draw_rectangle(self, rectangle: &Rectangle, clips: &[PhysicalRect]) {
        unsafe { (self.vtable.draw_rectangle)(self.data, rectangle, clips) }
    }

    #[inline]
    pub fn draw_image(self, image: &Image<'_>, clips: &[PhysicalRect]) {
        unsafe { (self.vtable.draw_image)(self.data, image, clips) }
    }

    #[inline]
    pub fn draw_text(self, request: &TextRequest<'_>, clips: &[PhysicalRect]) {
        unsafe { (self.vtable.draw_text)(self.data, request, clips) }
    }
}
