use std::{
    ptr::NonNull,
    simd::{num::SimdUint, Simd},
};

use blit::color::Color;
use blit_cpu::{Pixel, PixelBuffer, PremultipliedRgbaColor};

type U32x8 = Simd<u32, 8>;

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct DesktopPixel(u32);

pub struct DesktopBuffer {
    pixels: NonNull<DesktopPixel>,
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
    type Pixel = DesktopPixel;

    fn width(&self) -> usize { self.width }

    fn height(&self) -> usize { self.height }

    fn line_mut(&mut self, line: usize) -> &mut [Self::Pixel] {
        assert!(line < self.height);
        // safety: set provides width * height writable pixels for the duration of rendering
        unsafe { std::slice::from_raw_parts_mut(self.pixels.as_ptr().add(line * self.width), self.width) }
    }
}

impl Pixel for DesktopPixel {
    fn blend_translucent(&mut self, color: PremultipliedRgbaColor) {
        let inverse = 255 - color.alpha as u32;
        let red = ((self.0 >> 16) & 0xff) * inverse / 255 + color.red as u32;
        let green = ((self.0 >> 8) & 0xff) * inverse / 255 + color.green as u32;
        let blue = (self.0 & 0xff) * inverse / 255 + color.blue as u32;
        self.0 = red << 16 | green << 8 | blue;
    }

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self((red as u32) << 16 | (green as u32) << 8 | blue as u32)
    }

    fn blend_slice(pixels: &mut [Self], color: PremultipliedRgbaColor) {
        match color.alpha {
            0 => return,
            255 => {
                pixels.fill(Self::from_rgb(color.red, color.green, color.blue));
                return;
            }
            _ => {}
        }

        let (chunks, tail) = as_u32_slice_mut(pixels).as_chunks_mut::<8>();
        let alpha = U32x8::splat(color.alpha as u32);
        let red = U32x8::splat(color.red as u32);
        let green = U32x8::splat(color.green as u32);
        let blue = U32x8::splat(color.blue as u32);
        for pixels in chunks {
            let blended = blend(U32x8::from_array(*pixels), alpha, red, green, blue);
            *pixels = blended.to_array();
        }
        for pixel in tail {
            let mut destination = DesktopPixel(*pixel);
            destination.blend(color);
            *pixel = destination.0;
        }
    }

    fn blend_alpha_slice(pixels: &mut [Self], color: Color, alpha: &[u8]) {
        if color.alpha == 0 {
            return;
        }
        let len = pixels.len().min(alpha.len());
        let pixels = &mut as_u32_slice_mut(pixels)[..len];
        let alpha = &alpha[..len];
        let chunks = len / 8;
        let color_alpha = U32x8::splat(color.alpha as u32);
        let color_red = U32x8::splat(color.red as u32);
        let color_green = U32x8::splat(color.green as u32);
        let color_blue = U32x8::splat(color.blue as u32);
        for index in 0..chunks {
            let start = index * 8;
            let coverage = Simd::<u8, 8>::from_slice(&alpha[start..start + 8]).cast::<u32>();
            let source_alpha = divide_by_255(color_alpha * coverage);
            let red = divide_by_255(color_red * source_alpha);
            let green = divide_by_255(color_green * source_alpha);
            let blue = divide_by_255(color_blue * source_alpha);
            let destination = U32x8::from_slice(&pixels[start..start + 8]);
            blend(destination, source_alpha, red, green, blue).copy_to_slice(&mut pixels[start..start + 8]);
        }
        for index in chunks * 8..len {
            let mut destination = DesktopPixel(pixels[index]);
            destination.blend(PremultipliedRgbaColor::new(color, alpha[index]));
            pixels[index] = destination.0;
        }
    }
}

fn as_u32_slice_mut(pixels: &mut [DesktopPixel]) -> &mut [u32] {
    // safety: DesktopPixel is transparent over u32
    unsafe { std::slice::from_raw_parts_mut(pixels.as_mut_ptr().cast(), pixels.len()) }
}

fn divide_by_255(value: U32x8) -> U32x8 {
    (value + U32x8::splat(1) + (value >> U32x8::splat(8))) >> U32x8::splat(8)
}

fn blend(destination: U32x8, alpha: U32x8, red: U32x8, green: U32x8, blue: U32x8) -> U32x8 {
    let inverse = U32x8::splat(255) - alpha;
    let red = divide_by_255(((destination >> U32x8::splat(16)) & U32x8::splat(0xff)) * inverse) + red;
    let green = divide_by_255(((destination >> U32x8::splat(8)) & U32x8::splat(0xff)) * inverse) + green;
    let blue = divide_by_255((destination & U32x8::splat(0xff)) * inverse) + blue;
    red << U32x8::splat(16) | green << U32x8::splat(8) | blue
}
