use std::ops::Range;

use blit::color::Color;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct PremultipliedRgbaColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl PremultipliedRgbaColor {
    pub fn new(color: Color, coverage: u8) -> Self {
        let alpha = color.alpha as u16 * coverage as u16 / 255;
        Self {
            alpha: alpha as u8,
            red: (color.red as u16 * alpha / 255) as u8,
            green: (color.green as u16 * alpha / 255) as u8,
            blue: (color.blue as u16 * alpha / 255) as u8,
        }
    }

    pub fn with_opacity(color: Color, opacity: f32) -> Self {
        Self::new(color, (opacity.clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    pub fn coverage(self, coverage: u32) -> Self {
        Self {
            alpha: (self.alpha as u32 * coverage / 255) as u8,
            red: (self.red as u32 * coverage / 255) as u8,
            green: (self.green as u32 * coverage / 255) as u8,
            blue: (self.blue as u32 * coverage / 255) as u8,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Rgb8Pixel {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

pub trait Pixel: Copy {
    /// composites `color`, skipping transparent colors and replacing opaque pixels
    #[inline(always)]
    fn blend(&mut self, color: PremultipliedRgbaColor) {
        match color.alpha {
            0 => {}
            255 => *self = Self::from_rgb(color.red, color.green, color.blue),
            _ => self.blend_translucent(color),
        }
    }

    /// composites `color`; `blend` only calls this when alpha is in `1..=254`
    fn blend_translucent(&mut self, color: PremultipliedRgbaColor);

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self;

    fn background() -> Self { Self::from_rgb(0, 0, 0) }

    fn blend_slice(pixels: &mut [Self], color: PremultipliedRgbaColor) {
        match color.alpha {
            0 => {}
            255 => pixels.fill(Self::from_rgb(color.red, color.green, color.blue)),
            _ => pixels.iter_mut().for_each(|pixel| pixel.blend(color)),
        }
    }

    fn blend_alpha_slice(pixels: &mut [Self], color: Color, alpha: &[u8]) {
        if color.alpha == 0 {
            return;
        }
        for (pixel, alpha) in pixels.iter_mut().zip(alpha) {
            pixel.blend(PremultipliedRgbaColor::new(color, *alpha));
        }
    }

    fn blend_texture_slice_rgb(pixels: &mut [Self], source: &[Rgb8Pixel]) {
        for (pixel, source) in pixels.iter_mut().zip(source) {
            *pixel = Self::from_rgb(source.red, source.green, source.blue);
        }
    }

    fn blend_texture_slice_rgba(pixels: &mut [Self], source: &[PremultipliedRgbaColor], opacity: u8) {
        for (pixel, source) in pixels.iter_mut().zip(source) {
            pixel.blend(if opacity == 255 { *source } else { source.coverage(opacity as u32) });
        }
    }

    /// copies premultiplied texture pixels whose alpha is known to be opaque
    fn copy_texture_slice_rgba(pixels: &mut [Self], source: &[PremultipliedRgbaColor]) {
        for (pixel, source) in pixels.iter_mut().zip(source) {
            *pixel = Self::from_rgb(source.red, source.green, source.blue);
        }
    }

    fn blend_texture_slice_alpha(pixels: &mut [Self], color: Color, alpha: &[u8]) {
        Self::blend_alpha_slice(pixels, color, alpha);
    }
}

impl Pixel for u32 {
    fn blend_translucent(&mut self, color: PremultipliedRgbaColor) {
        let inverse = 255 - color.alpha as u32;
        let red = ((*self >> 16) & 0xff) * inverse / 255 + color.red as u32;
        let green = ((*self >> 8) & 0xff) * inverse / 255 + color.green as u32;
        let blue = (*self & 0xff) * inverse / 255 + color.blue as u32;
        *self = red << 16 | green << 8 | blue;
    }

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        (red as u32) << 16 | (green as u32) << 8 | blue as u32
    }
}

/// a borrowed scanline span whose first pixel is at the absolute x coordinate
pub struct PixelSpan<'a, P> {
    pub x: i32,
    pub pixels: &'a mut [P],
}

pub trait PixelBuffer {
    type Pixel: Pixel;

    fn x_offset(&self) -> usize { 0 }

    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn line_mut(&mut self, line: usize) -> &mut [Self::Pixel];

    fn process_line(&mut self, line: usize, range: Range<usize>, process: impl FnOnce(&mut [Self::Pixel])) {
        process(&mut self.line_mut(line)[range]);
    }
}

pub struct VecBuffer<P> {
    pixels: Vec<P>,
    width: usize,
    height: usize,
}

impl<P: Pixel> VecBuffer<P> {
    pub fn new(width: usize, height: usize) -> Self {
        Self { pixels: vec![P::background(); width * height], width, height }
    }

    pub fn pixels(&self) -> &[P] { &self.pixels }

    pub fn pixels_mut(&mut self) -> &mut [P] { &mut self.pixels }
}

impl<P: Pixel> PixelBuffer for VecBuffer<P> {
    type Pixel = P;

    fn width(&self) -> usize { self.width }

    fn height(&self) -> usize { self.height }

    fn line_mut(&mut self, line: usize) -> &mut [P] {
        let start = line * self.width;
        &mut self.pixels[start..start + self.width]
    }
}
