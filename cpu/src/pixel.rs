use std::{
    ops::Range,
    simd::{Simd, num::SimdUint},
};

use blit::color::Color;

type U16x8 = Simd<u16, 8>;
type U32x8 = Simd<u32, 8>;

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

    fn background() -> Self {
        Self::from_rgb(0, 0, 0)
    }

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

    fn blend_texture_slice_rgba(
        pixels: &mut [Self],
        source: &[PremultipliedRgbaColor],
        opacity: u8,
    ) {
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

impl<const RED: u8, const GREEN: u8, const BLUE: u8, const ALPHA: u32>
    PackedPixel<RED, GREEN, BLUE, ALPHA>
{
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl<const RED: u8, const GREEN: u8, const BLUE: u8, const ALPHA: u32> Pixel
    for PackedPixel<RED, GREEN, BLUE, ALPHA>
{
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
        let alpha = U16x8::splat(color.alpha as u16);
        let red = U16x8::splat(color.red as u16);
        let green = U16x8::splat(color.green as u16);
        let blue = U16x8::splat(color.blue as u16);
        for pixels in chunks {
            let destination = U32x8::from_array((*pixels).map(|pixel| pixel.0));
            *pixels = blend::<RED, GREEN, BLUE, ALPHA>(destination, alpha, red, green, blue)
                .to_array()
                .map(Self);
        }
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
        let color_alpha = U16x8::splat(color.alpha as u16);
        let color_red = U16x8::splat(color.red as u16);
        let color_green = U16x8::splat(color.green as u16);
        let color_blue = U16x8::splat(color.blue as u16);
        for (pixels, alpha) in pixel_chunks.iter_mut().zip(alpha_chunks) {
            let coverage = Simd::<u8, 8>::from_array(*alpha).cast::<u16>();
            let source_alpha = if color.alpha == 255 {
                coverage
            } else {
                divide_by_255(color_alpha * coverage)
            };
            let red = divide_by_255(color_red * source_alpha);
            let green = divide_by_255(color_green * source_alpha);
            let blue = divide_by_255(color_blue * source_alpha);
            let destination = U32x8::from_array((*pixels).map(|pixel| pixel.0));
            *pixels = blend::<RED, GREEN, BLUE, ALPHA>(destination, source_alpha, red, green, blue)
                .to_array()
                .map(Self);
        }
        for (pixel, alpha) in pixel_tail.iter_mut().zip(alpha_tail) {
            pixel.blend(PremultipliedRgbaColor::new(color, *alpha));
        }
    }

    fn blend_texture_slice_rgba(
        pixels: &mut [Self],
        source: &[PremultipliedRgbaColor],
        opacity: u8,
    ) {
        if opacity == 0 {
            return;
        }
        let len = pixels.len().min(source.len());
        let (pixel_chunks, pixel_tail) = pixels[..len].as_chunks_mut::<8>();
        let (source_chunks, source_tail) = source[..len].as_chunks::<8>();
        let opacity_vector = U16x8::splat(opacity as u16);
        for (pixels, source) in pixel_chunks.iter_mut().zip(source_chunks) {
            let mut alpha = U16x8::from_array(source.map(|pixel| pixel.alpha as u16));
            let mut red = U16x8::from_array(source.map(|pixel| pixel.red as u16));
            let mut green = U16x8::from_array(source.map(|pixel| pixel.green as u16));
            let mut blue = U16x8::from_array(source.map(|pixel| pixel.blue as u16));
            if opacity != 255 {
                alpha = divide_by_255(alpha * opacity_vector);
                red = divide_by_255(red * opacity_vector);
                green = divide_by_255(green * opacity_vector);
                blue = divide_by_255(blue * opacity_vector);
            }
            let destination = U32x8::from_array((*pixels).map(|pixel| pixel.0));
            *pixels = blend::<RED, GREEN, BLUE, ALPHA>(destination, alpha, red, green, blue)
                .to_array()
                .map(Self);
        }
        for (pixel, source) in pixel_tail.iter_mut().zip(source_tail) {
            pixel.blend(if opacity == 255 {
                *source
            } else {
                source.coverage(opacity as u32)
            });
        }
    }
}

fn divide_by_255(value: U16x8) -> U16x8 {
    (value + U16x8::splat(1) + (value >> U16x8::splat(8))) >> U16x8::splat(8)
}

fn blend<const RED: u8, const GREEN: u8, const BLUE: u8, const ALPHA: u32>(
    destination: U32x8,
    alpha: U16x8,
    red: U16x8,
    green: U16x8,
    blue: U16x8,
) -> U32x8 {
    let inverse = U16x8::splat(255) - alpha;
    let red = divide_by_255(
        ((destination >> U32x8::splat(RED as u32)) & U32x8::splat(0xff)).cast::<u16>() * inverse,
    ) + red;
    let green = divide_by_255(
        ((destination >> U32x8::splat(GREEN as u32)) & U32x8::splat(0xff)).cast::<u16>() * inverse,
    ) + green;
    let blue = divide_by_255(
        ((destination >> U32x8::splat(BLUE as u32)) & U32x8::splat(0xff)).cast::<u16>() * inverse,
    ) + blue;
    let output = red.cast::<u32>() << U32x8::splat(RED as u32)
        | green.cast::<u32>() << U32x8::splat(GREEN as u32)
        | blue.cast::<u32>() << U32x8::splat(BLUE as u32);
    if ALPHA == 0 {
        output
    } else {
        let shift = U32x8::splat(ALPHA.trailing_zeros());
        let destination_alpha = ((destination & U32x8::splat(ALPHA)) >> shift).cast::<u16>();
        output | (divide_by_255(destination_alpha * inverse) + alpha).cast::<u32>() << shift
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

    fn process_line(
        &mut self,
        line: usize,
        range: Range<usize>,
        process: impl FnOnce(&mut [Self::Pixel]),
    ) {
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
        let len = width
            .checked_mul(height)
            .expect("pixel buffer dimensions overflow");
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
        let len = width
            .checked_mul(height)
            .expect("pixel buffer dimensions overflow");
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
                    Color::from_rgba8(
                        index as u8 * 17,
                        220 - index as u8 * 11,
                        index as u8 * 7,
                        181,
                    ),
                    index as u8 * 23,
                ));
                pixel
            });
            let mut expected = actual;
            let alpha = [0, 1, 32, 64, 96, 127, 128, 160, 224, 254, 255];
            let color = Color::from_rgba8(190, 80, 230, 177);

            P::blend_alpha_slice(&mut actual, color, &alpha);
            for (pixel, alpha) in expected.iter_mut().zip(alpha) {
                pixel.blend(PremultipliedRgbaColor::new(color, alpha));
            }
            assert_eq!(actual, expected);

            let color = PremultipliedRgbaColor::new(Color::from_rgba8(20, 210, 70, 143), 255);
            P::blend_slice(&mut actual, color);
            for pixel in &mut expected {
                pixel.blend(color);
            }
            assert_eq!(actual, expected);

            let source: [PremultipliedRgbaColor; 11] = std::array::from_fn(|index| {
                PremultipliedRgbaColor::new(
                    Color::from_rgba8(
                        230 - index as u8 * 13,
                        index as u8 * 19,
                        40 + index as u8 * 7,
                        220,
                    ),
                    20 + index as u8 * 21,
                )
            });
            P::blend_texture_slice_rgba(&mut actual, &source, 173);
            for (pixel, source) in expected.iter_mut().zip(source) {
                pixel.blend(source.coverage(173));
            }
            assert_eq!(actual, expected);
        }

        check::<Xrgb8888>();
        check::<Argb8888>();
        check::<Rgba8888>();
    }
}
