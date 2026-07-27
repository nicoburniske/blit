use std::ptr::NonNull;

use blit_cpu::{PixelBuffer, Xrgb8888};

pub struct DesktopBuffer {
    pixels: NonNull<Xrgb8888>,
    width: usize,
    height: usize,
}

impl DesktopBuffer {
    pub fn new(width: usize, height: usize) -> Self { Self { pixels: NonNull::dangling(), width, height } }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.pixels = NonNull::dangling();
        self.width = width;
        self.height = height;
    }

    pub fn set(&mut self, pixels: &mut [u32]) {
        assert!(pixels.len() >= self.width * self.height);
        self.pixels = NonNull::new(pixels.as_mut_ptr().cast()).expect("softbuffer pixels");
    }
}

impl PixelBuffer for DesktopBuffer {
    type Pixel = Xrgb8888;

    fn width(&self) -> usize { self.width }

    fn height(&self) -> usize { self.height }

    fn line_mut(&mut self, line: usize) -> &mut [Self::Pixel] {
        assert!(line < self.height);
        // safety: set provides writable u32 pixels and Xrgb8888 is transparent over u32
        unsafe { std::slice::from_raw_parts_mut(self.pixels.as_ptr().add(line * self.width), self.width) }
    }
}
