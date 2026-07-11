use std::{marker::PhantomData, ptr::NonNull, rc::Rc};

use tiny_skia::PixmapMut;

use crate::{Rect, TextMetrics, TextRequest};

#[derive(Clone, Copy)]
pub struct Platform {
    data: NonNull<()>,
    vtable: &'static PlatformVTable,
    not_send_or_sync: PhantomData<Rc<()>>,
}

pub struct PlatformVTable {
    pub measure_text: unsafe fn(NonNull<()>, &TextRequest<'_>) -> TextMetrics,
    pub draw_text: unsafe fn(NonNull<()>, &mut PixmapMut<'_>, &TextRequest<'_>, &[Rect]),
}

impl Platform {
    /// `data` must remain valid for every use of the returned platform.
    pub unsafe fn new(data: NonNull<()>, vtable: &'static PlatformVTable) -> Self {
        Self {
            data,
            vtable,
            not_send_or_sync: PhantomData,
        }
    }

    pub fn measure_text(self, request: &TextRequest<'_>) -> TextMetrics {
        // safety: upheld by the constructor
        unsafe { (self.vtable.measure_text)(self.data, request) }
    }

    pub fn draw_text(self, pixels: &mut PixmapMut<'_>, request: &TextRequest<'_>, clips: &[Rect]) {
        // safety: upheld by the constructor
        unsafe { (self.vtable.draw_text)(self.data, pixels, request, clips) }
    }
}
