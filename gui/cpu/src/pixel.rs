use std::ops::Range;

use blit_gui::color::Color;
use fearless_simd::{Level, dispatch, i32x8, prelude::*, u32x8};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct PremultipliedRgbaColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl PremultipliedRgbaColor {
    pub fn over(self, background: Self) -> Self {
        let inverse = 255 - self.alpha as u16;
        let channel = |front: u8, back: u8| front + (back as u16 * inverse / 255) as u8;
        Self {
            red: channel(self.red, background.red),
            green: channel(self.green, background.green),
            blue: channel(self.blue, background.blue),
            alpha: channel(self.alpha, background.alpha),
        }
    }

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
        if coverage == 255 {
            return self;
        }
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

    fn background() -> Self {
        Self::from_rgb(0, 0, 0)
    }

    fn write_opaque_gradient(pixels: &mut [Self], color: [f32; 3], step: [f32; 3]) {
        write_opaque_gradient_scalar(pixels, color, step);
    }

    fn blend_slice(pixels: &mut [Self], color: PremultipliedRgbaColor) {
        match color.alpha {
            0 => {}
            255 => pixels.fill(Self::from_rgb(color.red, color.green, color.blue)),
            _ => pixels.iter_mut().for_each(|pixel| pixel.blend(color)),
        }
    }

    fn blend_solid_pair(pixels: &mut [Self], first: PremultipliedRgbaColor, second: PremultipliedRgbaColor) {
        Self::blend_slice(pixels, first);
        Self::blend_slice(pixels, second);
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
            pixel.blend(if opacity == 255 {
                *source
            } else {
                source.coverage(opacity as u32)
            });
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct PackedPixel<const RED: u8, const GREEN: u8, const BLUE: u8, const ALPHA: u32>(u32);

pub type Xrgb8888 = PackedPixel<16, 8, 0, 0>;
pub type Argb8888 = PackedPixel<16, 8, 0, 0xff00_0000>;
pub type Rgba8888 = PackedPixel<24, 16, 8, 0x0000_00ff>;

impl<const RED: u8, const GREEN: u8, const BLUE: u8, const ALPHA: u32> PackedPixel<RED, GREEN, BLUE, ALPHA> {
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl<const RED: u8, const GREEN: u8, const BLUE: u8, const ALPHA: u32> Pixel for PackedPixel<RED, GREEN, BLUE, ALPHA> {
    fn blend_translucent(&mut self, color: PremultipliedRgbaColor) {
        let inverse = 255 - color.alpha as u32;
        let red = ((self.0 >> RED) & 0xff) * inverse / 255 + color.red as u32;
        let green = ((self.0 >> GREEN) & 0xff) * inverse / 255 + color.green as u32;
        let blue = ((self.0 >> BLUE) & 0xff) * inverse / 255 + color.blue as u32;
        let alpha = if ALPHA == 0 {
            0
        } else {
            let shift = ALPHA.trailing_zeros();
            ((((self.0 & ALPHA) >> shift) * inverse / 255 + color.alpha as u32) << shift) & ALPHA
        };
        self.0 = red << RED | green << GREEN | blue << BLUE | alpha;
    }

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self((red as u32) << RED | (green as u32) << GREEN | (blue as u32) << BLUE | ALPHA)
    }

    fn background() -> Self {
        Self(0)
    }

    fn write_opaque_gradient(pixels: &mut [Self], color: [f32; 3], step: [f32; 3]) {
        if pixels.len() < 8 {
            write_opaque_gradient_scalar(pixels, color, step);
            return;
        }
        const FRACTION_BITS: u32 = 20;
        const SCALE: f32 = (1 << FRACTION_BITS) as f32;
        const HALF: i32 = 1 << (FRACTION_BITS - 1);

        let fixed = |value: f32| {
            let value = value * SCALE;
            (value + if value < 0.0 { -0.5 } else { 0.5 }) as i32
        };
        let mut color = color.map(fixed);
        let step = step.map(fixed);
        let (chunks, tail) = pixels.as_chunks_mut::<8>();
        dispatch!(Level::new(), simd => {
            let lanes = i32x8::simd_from(simd, [0, 1, 2, 3, 4, 5, 6, 7]);
            let zero = i32x8::splat(simd, 0);
            let maximum = i32x8::splat(simd, 255 << FRACTION_BITS);
            for pixels in chunks.iter_mut() {
                let channel = |index: usize| {
                    ((lanes * step[index] + color[index].wrapping_add(HALF)).max(zero).min(maximum) >> FRACTION_BITS)
                        .bitcast::<u32x8<_>>()
                };
                let packed = channel(0) << RED as u32 | channel(1) << GREEN as u32 | channel(2) << BLUE as u32 | ALPHA;
                *pixels = <[u32; 8]>::from(packed).map(Self);
                for index in 0..3 {
                    color[index] = color[index].wrapping_add(step[index].wrapping_mul(8));
                }
            }
        });
        for pixel in tail {
            let channel = |index: usize| ((color[index] + HALF) >> FRACTION_BITS).clamp(0, 255) as u8;
            *pixel = Self::from_rgb(channel(0), channel(1), channel(2));
            for index in 0..3 {
                color[index] = color[index].wrapping_add(step[index]);
            }
        }
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

        let (chunks, tail) = pixels.as_chunks_mut::<8>();
        dispatch!(Level::new(), simd => {
            let alpha = u32x8::splat(simd, color.alpha as u32);
            let red = u32x8::splat(simd, color.red as u32);
            let green = u32x8::splat(simd, color.green as u32);
            let blue = u32x8::splat(simd, color.blue as u32);
            for pixels in chunks.iter_mut() {
                let destination = u32x8::simd_from(simd, (*pixels).map(|pixel| pixel.0));
                *pixels = <[u32; 8]>::from(blend::<_, RED, GREEN, BLUE, ALPHA>(destination, alpha, red, green, blue))
                    .map(Self);
            }
        });
        for pixel in tail {
            pixel.blend(color);
        }
    }

    fn blend_alpha_slice(pixels: &mut [Self], color: Color, alpha: &[u8]) {
        if color.alpha == 0 {
            return;
        }
        let len = pixels.len().min(alpha.len());
        let (pixel_chunks, pixel_tail) = pixels[..len].as_chunks_mut::<8>();
        let (alpha_chunks, alpha_tail) = alpha[..len].as_chunks::<8>();
        dispatch!(Level::new(), simd => {
            for (pixels, alpha) in pixel_chunks.iter_mut().zip(alpha_chunks) {
                let coverage = u32x8::simd_from(simd, alpha.map(u32::from));
                let source_alpha = if color.alpha == 255 {
                    coverage
                } else {
                    divide_by_255(coverage * color.alpha as u32)
                };
                let red = divide_by_255(source_alpha * color.red as u32);
                let green = divide_by_255(source_alpha * color.green as u32);
                let blue = divide_by_255(source_alpha * color.blue as u32);
                let destination = u32x8::simd_from(simd, (*pixels).map(|pixel| pixel.0));
                *pixels =
                    <[u32; 8]>::from(blend::<_, RED, GREEN, BLUE, ALPHA>(destination, source_alpha, red, green, blue))
                        .map(Self);
            }
        });
        for (pixel, alpha) in pixel_tail.iter_mut().zip(alpha_tail) {
            pixel.blend(PremultipliedRgbaColor::new(color, *alpha));
        }
    }

    fn blend_texture_slice_rgba(pixels: &mut [Self], source: &[PremultipliedRgbaColor], opacity: u8) {
        if opacity == 0 {
            return;
        }
        let len = pixels.len().min(source.len());
        let (pixel_chunks, pixel_tail) = pixels[..len].as_chunks_mut::<8>();
        let (source_chunks, source_tail) = source[..len].as_chunks::<8>();
        dispatch!(Level::new(), simd => {
            for (pixels, source) in pixel_chunks.iter_mut().zip(source_chunks) {
                let mut alpha = u32x8::simd_from(simd, source.map(|pixel| pixel.alpha as u32));
                let mut red = u32x8::simd_from(simd, source.map(|pixel| pixel.red as u32));
                let mut green = u32x8::simd_from(simd, source.map(|pixel| pixel.green as u32));
                let mut blue = u32x8::simd_from(simd, source.map(|pixel| pixel.blue as u32));
                if opacity != 255 {
                    alpha = divide_by_255(alpha * opacity as u32);
                    red = divide_by_255(red * opacity as u32);
                    green = divide_by_255(green * opacity as u32);
                    blue = divide_by_255(blue * opacity as u32);
                }
                let destination = u32x8::simd_from(simd, (*pixels).map(|pixel| pixel.0));
                *pixels = <[u32; 8]>::from(blend::<_, RED, GREEN, BLUE, ALPHA>(destination, alpha, red, green, blue))
                    .map(Self);
            }
        });
        for (pixel, source) in pixel_tail.iter_mut().zip(source_tail) {
            pixel.blend(if opacity == 255 {
                *source
            } else {
                source.coverage(opacity as u32)
            });
        }
    }
}

#[inline(always)]
fn divide_by_255<S: Simd>(value: u32x8<S>) -> u32x8<S> {
    (value + 1 + (value >> 8)) >> 8
}

#[inline(always)]
fn write_opaque_gradient_scalar<P: Pixel>(pixels: &mut [P], mut color: [f32; 3], step: [f32; 3]) {
    for pixel in pixels {
        *pixel = P::from_rgb((color[0] + 0.5) as u8, (color[1] + 0.5) as u8, (color[2] + 0.5) as u8);
        for channel in 0..3 {
            color[channel] += step[channel];
        }
    }
}

#[inline(always)]
fn blend<S: Simd, const RED: u8, const GREEN: u8, const BLUE: u8, const ALPHA: u32>(
    destination: u32x8<S>,
    alpha: u32x8<S>,
    red: u32x8<S>,
    green: u32x8<S>,
    blue: u32x8<S>,
) -> u32x8<S> {
    let inverse = alpha.simd.splat_u32x8(255) - alpha;
    let red = divide_by_255(((destination >> RED as u32) & 0xff) * inverse) + red;
    let green = divide_by_255(((destination >> GREEN as u32) & 0xff) * inverse) + green;
    let blue = divide_by_255(((destination >> BLUE as u32) & 0xff) * inverse) + blue;
    let output = red << RED as u32 | green << GREEN as u32 | blue << BLUE as u32;
    if ALPHA == 0 {
        output
    } else {
        let shift = ALPHA.trailing_zeros();
        let destination_alpha = (destination & ALPHA) >> shift;
        output | (divide_by_255(destination_alpha * inverse) + alpha) << shift
    }
}

/// a borrowed scanline span whose first pixel is at the absolute x coordinate
pub struct PixelSpan<'a, P> {
    pub x: i32,
    pub pixels: &'a mut [P],
}

pub trait PixelBuffer {
    type Pixel: Pixel;

    fn x_offset(&self) -> usize {
        0
    }

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
        let len = width.checked_mul(height).expect("pixel buffer dimensions overflow");
        Self {
            pixels: vec![P::background(); len],
            width,
            height,
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        if self.width == width && self.height == height {
            return;
        }
        let len = width.checked_mul(height).expect("pixel buffer dimensions overflow");
        self.pixels.clear();
        self.pixels.resize(len, P::background());
        self.width = width;
        self.height = height;
    }

    pub fn pixels(&self) -> &[P] {
        &self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [P] {
        &mut self.pixels
    }
}

impl<P: Pixel> PixelBuffer for VecBuffer<P> {
    type Pixel = P;

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn line_mut(&mut self, line: usize) -> &mut [P] {
        let start = line * self.width;
        &mut self.pixels[start..start + self.width]
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn packed_formats_place_channels_and_default_to_transparent() {
        assert_eq!(Xrgb8888::from_rgb(0x12, 0x34, 0x56).raw(), 0x0012_3456);
        assert_eq!(Argb8888::from_rgb(0x12, 0x34, 0x56).raw(), 0xff12_3456);
        assert_eq!(Rgba8888::from_rgb(0x12, 0x34, 0x56).raw(), 0x1234_56ff);
        assert_eq!(Argb8888::default().raw(), 0);
        assert_eq!(Rgba8888::default().raw(), 0);
        assert_eq!(Argb8888::background().raw(), 0);
        assert_eq!(Rgba8888::background().raw(), 0);
    }

    #[test]
    fn alpha_formats_composite_translucent_pixels() {
        let color = PremultipliedRgbaColor {
            red: 100,
            green: 50,
            blue: 25,
            alpha: 128,
        };
        let mut argb = Argb8888::default();
        argb.blend(color);
        assert_eq!(argb.raw(), 0x8064_3219);
        argb.blend(color);
        assert_eq!(argb.raw(), 0xbf95_4a25);

        let mut rgba = Rgba8888::default();
        rgba.blend(color);
        assert_eq!(rgba.raw(), 0x6432_1980);
        rgba.blend(color);
        assert_eq!(rgba.raw(), 0x954a_25bf);
    }

    #[test]
    fn simd_blending_matches_scalar_blending_for_each_format() {
        fn check<P: Pixel + std::fmt::Debug + PartialEq>() {
            let mut actual: [P; 11] = std::array::from_fn(|index| {
                let mut pixel = P::background();
                pixel.blend(PremultipliedRgbaColor::new(
                    Color::rgba(index as u8 * 17, 220 - index as u8 * 11, index as u8 * 7, 181),
                    index as u8 * 23,
                ));
                pixel
            });
            let mut expected = actual;
            let alpha = [0, 1, 32, 64, 96, 127, 128, 160, 224, 254, 255];
            let color = Color::rgba(190, 80, 230, 177);

            P::blend_alpha_slice(&mut actual, color, &alpha);
            for (pixel, alpha) in expected.iter_mut().zip(alpha) {
                pixel.blend(PremultipliedRgbaColor::new(color, alpha));
            }
            assert_eq!(actual, expected);

            let color = PremultipliedRgbaColor::new(Color::rgba(20, 210, 70, 143), 255);
            P::blend_slice(&mut actual, color);
            for pixel in &mut expected {
                pixel.blend(color);
            }
            assert_eq!(actual, expected);

            let source: [PremultipliedRgbaColor; 11] = std::array::from_fn(|index| {
                PremultipliedRgbaColor::new(
                    Color::rgba(230 - index as u8 * 13, index as u8 * 19, 40 + index as u8 * 7, 220),
                    20 + index as u8 * 21,
                )
            });
            P::blend_texture_slice_rgba(&mut actual, &source, 173);
            for (pixel, source) in expected.iter_mut().zip(source) {
                pixel.blend(source.coverage(173));
            }
            assert_eq!(actual, expected);

            let mut color = [12.25, 240.75, -0.2];
            let step = [4.125, -3.75, 1.0];
            P::write_opaque_gradient(&mut actual, color, step);
            for pixel in &mut expected {
                *pixel = P::from_rgb((color[0] + 0.5) as u8, (color[1] + 0.5) as u8, (color[2] + 0.5) as u8);
                for channel in 0..3 {
                    color[channel] += step[channel];
                }
            }
            assert_eq!(actual, expected);
        }

        check::<Xrgb8888>();
        check::<Argb8888>();
        check::<Rgba8888>();
    }
}
