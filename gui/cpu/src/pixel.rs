use std::ops::Range;

use blit::ScratchBuffer;
use blit_gui::color::Color;
use fearless_simd::{Level, Simd, dispatch, f32x8, i32x8, prelude::*, u32x8};

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

pub trait Pixel: Copy + 'static {
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

    /// constructs a pixel from premultiplied channels
    fn from_rgba(color: PremultipliedRgbaColor) -> Self {
        let mut pixel = Self::background();
        pixel.blend(color);
        pixel
    }

    /// reads premultiplied channels including alpha for transparent formats
    fn to_rgba(self) -> PremultipliedRgbaColor;

    fn background() -> Self {
        Self::from_rgb(0, 0, 0)
    }

    /// blurs channel arrays in place with reusable scratch storage
    fn blur<const N: usize>(pixels: &mut [[u8; N]], width: usize, height: usize, radius: u32, scratch: &ScratchBuffer) {
        assert_eq!(
            pixels.len(),
            width.checked_mul(height).expect("blur dimensions overflow")
        );
        if radius == 0 || width == 0 || height == 0 || N == 0 {
            return;
        }
        let radius = radius.min(Self::MAX_BLUR_RADIUS) as usize;
        let mut stack = scratch.alloc::<i32>((radius * 2 + 1) * N);
        stack.resize((radius * 2 + 1) * N, 0);
        let stack = stack.as_chunks_mut::<N>().0;
        for row in 0..height {
            line(pixels, row * width, 1, width, radius, stack);
        }
        for column in 0..width {
            line(pixels, column, width, height, radius, stack);
        }

        fn line<const N: usize>(
            pixels: &mut [[u8; N]],
            start: usize,
            step: usize,
            length: usize,
            radius: usize,
            stack: &mut [[i32; N]],
        ) {
            let div = stack.len();
            let scale = 1.0 / ((radius + 1) * (radius + 1)) as f32;
            let first = pixels[start].map(i32::from);
            let mut sum = [0; N];
            let mut sum_in = [0; N];
            let mut sum_out = [0; N];
            for (i, slot) in stack.iter_mut().take(radius + 1).enumerate() {
                *slot = first;
                for channel in 0..N {
                    sum[channel] += first[channel] * (i as i32 + 1);
                    sum_out[channel] += first[channel];
                }
            }
            for i in 1..=radius {
                let source = pixels[start + i.min(length - 1) * step].map(i32::from);
                stack[i + radius] = source;
                for channel in 0..N {
                    sum[channel] += source[channel] * (radius + 1 - i) as i32;
                    sum_in[channel] += source[channel];
                }
            }
            let mut source = radius.min(length - 1);
            let mut position = radius;
            for output in 0..length {
                pixels[start + output * step] = sum.map(|value| (value as f32 * scale).round() as u8);
                let leaving = (position + div - radius) % div;
                source = (source + 1).min(length - 1);
                let input = pixels[start + source * step].map(i32::from);
                for channel in 0..N {
                    sum[channel] -= sum_out[channel];
                    sum_out[channel] -= stack[leaving][channel];
                }
                stack[leaving] = input;
                for channel in 0..N {
                    sum_in[channel] += input[channel];
                    sum[channel] += sum_in[channel];
                }
                position = (position + 1) % div;
                for channel in 0..N {
                    sum_out[channel] += stack[position][channel];
                    sum_in[channel] -= stack[position][channel];
                }
            }
        }
    }

    /// writes an rgb ramp with repeating rounding offsets added before byte conversion
    /// use `[0.5; 4]` for nearest rounding or varying offsets for dithering
    fn write_opaque_gradient(pixels: &mut [Self], color: [f32; 3], step: [f32; 3], rounding: [f32; 4]) {
        write_opaque_gradient_scalar(pixels, color, step, rounding);
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

    // weighted byte sums must fit the blur accumulator
    const MAX_BLUR_RADIUS: u32 = 2900;
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
    fn blur<const N: usize>(pixels: &mut [[u8; N]], width: usize, height: usize, radius: u32, scratch: &ScratchBuffer) {
        assert_eq!(
            pixels.len(),
            width.checked_mul(height).expect("blur dimensions overflow")
        );
        if radius == 0 || width == 0 || height == 0 || N == 0 {
            return;
        }
        let radius = radius.min(Self::MAX_BLUR_RADIUS) as usize;
        let mut stack = scratch.alloc::<i32>((radius * 2 + 1) * 8);
        stack.resize((radius * 2 + 1) * 8, 0);
        let stack = stack.as_chunks_mut::<8>().0;
        dispatch!(Level::new(), simd => {
            for channel in (0..N).step_by(8) {
                let channels = (N - channel).min(8);
                let batch = if N <= 8 { 8 / N } else { 8 / channels };
                for y in (0..height).step_by(batch) {
                    pass(simd, pixels, width, radius, stack,
                        #[inline(always)]
                        |pixels, x| std::array::from_fn(|lane| {
                            let channels = if N <= 8 { N } else { channels };
                            pixels[(y + lane / channels).min(height - 1) * width + x][channel + lane % channels] as i32
                        }),
                        #[inline(always)]
                        |pixels, x, output| {
                            if N <= 8 {
                                for row in 0..8 / N {
                                    if y + row < height {
                                        pixels[(y + row) * width + x] = std::array::from_fn(|i| output[row * N + i]);
                                    }
                                }
                            } else {
                                for row in 0..batch.min(height - y) {
                                    pixels[(y + row) * width + x][channel..channel + channels]
                                        .copy_from_slice(&output[row * channels..(row + 1) * channels]);
                                }
                            }
                        },
                    );
                }
                for x in (0..width).step_by(batch) {
                    pass(simd, pixels, height, radius, stack,
                        #[inline(always)]
                        |pixels, y| std::array::from_fn(|lane| {
                            let channels = if N <= 8 { N } else { channels };
                            pixels[y * width + (x + lane / channels).min(width - 1)][channel + lane % channels] as i32
                        }),
                        #[inline(always)]
                        |pixels, y, output| {
                            if N <= 8 {
                                for column in 0..8 / N {
                                    if x + column < width {
                                        pixels[y * width + x + column] = std::array::from_fn(|i| output[column * N + i]);
                                    }
                                }
                            } else {
                                for column in 0..batch.min(width - x) {
                                    pixels[y * width + x + column][channel..channel + channels]
                                        .copy_from_slice(&output[column * channels..(column + 1) * channels]);
                                }
                            }
                        },
                    );
                }
            }
        });

        #[inline(always)]
        fn pass<S: Simd, const N: usize>(
            simd: S,
            pixels: &mut [[u8; N]],
            length: usize,
            radius: usize,
            stack: &mut [[i32; 8]],
            mut load: impl FnMut(&[[u8; N]], usize) -> [i32; 8],
            mut store: impl FnMut(&mut [[u8; N]], usize, [u8; 8]),
        ) {
            let div = stack.len();
            let scale = 1.0 / ((radius + 1) * (radius + 1)) as f32;
            let first = load(pixels, 0);
            let first_vector = i32x8::simd_from(simd, first);
            let mut sum = i32x8::splat(simd, 0);
            let mut sum_in = i32x8::splat(simd, 0);
            let mut sum_out = i32x8::splat(simd, 0);

            for (i, slot) in stack.iter_mut().take(radius + 1).enumerate() {
                *slot = first;
                sum += first_vector * (i as i32 + 1);
                sum_out += first_vector;
            }
            for i in 1..=radius {
                let source = load(pixels, i.min(length - 1));
                stack[i + radius] = source;
                let source = i32x8::simd_from(simd, source);
                sum += source * (radius + 1 - i) as i32;
                sum_in += source;
            }

            let mut source = radius.min(length - 1);
            let mut position = radius;
            for output in 0..length {
                let blurred = (sum.to_float::<f32x8<S>>() * scale).round().to_int::<i32x8<S>>();
                store(pixels, output, <[i32; 8]>::from(blurred).map(|value| value as u8));

                sum -= sum_out;
                let mut leaving = position + div - radius;
                if leaving >= div {
                    leaving -= div;
                }
                sum_out -= i32x8::simd_from(simd, stack[leaving]);
                source = (source + 1).min(length - 1);
                let input = load(pixels, source);
                stack[leaving] = input;
                sum_in += i32x8::simd_from(simd, input);
                sum += sum_in;
                position += 1;
                if position >= div {
                    position = 0;
                }
                let entering = i32x8::simd_from(simd, stack[position]);
                sum_out += entering;
                sum_in -= entering;
            }
        }
    }

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

    fn from_rgba(color: PremultipliedRgbaColor) -> Self {
        let alpha = if ALPHA == 0 {
            0
        } else {
            (color.alpha as u32) << ALPHA.trailing_zeros()
        };
        Self((color.red as u32) << RED | (color.green as u32) << GREEN | (color.blue as u32) << BLUE | alpha)
    }

    fn to_rgba(self) -> PremultipliedRgbaColor {
        PremultipliedRgbaColor {
            red: (self.0 >> RED) as u8,
            green: (self.0 >> GREEN) as u8,
            blue: (self.0 >> BLUE) as u8,
            alpha: if ALPHA == 0 {
                255
            } else {
                ((self.0 & ALPHA) >> ALPHA.trailing_zeros()) as u8
            },
        }
    }

    fn background() -> Self {
        Self(0)
    }

    fn write_opaque_gradient(pixels: &mut [Self], color: [f32; 3], step: [f32; 3], rounding: [f32; 4]) {
        if pixels.len() < 8 {
            write_opaque_gradient_scalar(pixels, color, step, rounding);
            return;
        }
        const FRACTION_BITS: u32 = 20;
        const SCALE: f32 = (1 << FRACTION_BITS) as f32;

        let fixed = |value: f32| {
            let value = value * SCALE;
            (value + if value < 0.0 { -0.5 } else { 0.5 }) as i32
        };
        let mut color = color.map(fixed);
        let step = step.map(fixed);
        let rounding = rounding.map(fixed);
        let (chunks, tail) = pixels.as_chunks_mut::<8>();
        dispatch!(Level::new(), simd => {
            let lanes = i32x8::simd_from(simd, [0, 1, 2, 3, 4, 5, 6, 7]);
            let rounding = i32x8::simd_from(simd, std::array::from_fn(|i| rounding[i & 3]));
            let zero = i32x8::splat(simd, 0);
            let maximum = i32x8::splat(simd, 255 << FRACTION_BITS);
            for pixels in chunks.iter_mut() {
                let channel = |index: usize| {
                    ((lanes * step[index] + color[index] + rounding).max(zero).min(maximum) >> FRACTION_BITS)
                        .bitcast::<u32x8<_>>()
                };
                let packed = channel(0) << RED as u32 | channel(1) << GREEN as u32 | channel(2) << BLUE as u32 | ALPHA;
                *pixels = <[u32; 8]>::from(packed).map(Self);
                for index in 0..3 {
                    color[index] = color[index].wrapping_add(step[index].wrapping_mul(8));
                }
            }
        });
        for (i, pixel) in tail.iter_mut().enumerate() {
            let channel = |index: usize| ((color[index] + rounding[i & 3]) >> FRACTION_BITS).clamp(0, 255) as u8;
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
fn write_opaque_gradient_scalar<P: Pixel>(pixels: &mut [P], mut color: [f32; 3], step: [f32; 3], rounding: [f32; 4]) {
    for (i, pixel) in pixels.iter_mut().enumerate() {
        let channel = |index: usize| (color[index] + rounding[i & 3]) as u8;
        *pixel = P::from_rgb(channel(0), channel(1), channel(2));
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

    fn read_line(&mut self, line: usize, range: Range<usize>) -> &[Self::Pixel] {
        &self.line_mut(line)[range]
    }

    fn write_line(&mut self, line: usize, range: Range<usize>) -> &mut [Self::Pixel] {
        &mut self.line_mut(line)[range]
    }
}

pub struct VecBuffer<P> {
    pixels: Vec<P>,
    width: usize,
    height: usize,
}

impl<P: Pixel> VecBuffer<P> {
    pub fn new(width: usize, height: usize) -> Self {
        let Some(len) = width
            .checked_mul(height)
            .filter(|_| width <= i32::MAX as usize && height <= i32::MAX as usize)
        else {
            return Self {
                pixels: Vec::new(),
                width: 0,
                height: 0,
            };
        };
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
        let Some(len) = width
            .checked_mul(height)
            .filter(|_| width <= i32::MAX as usize && height <= i32::MAX as usize)
        else {
            return;
        };
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
        let color = PremultipliedRgbaColor {
            red: 40,
            green: 20,
            blue: 10,
            alpha: 80,
        };
        assert_eq!(Argb8888::from_rgba(color).to_rgba(), color);
        assert_eq!(Rgba8888::from_rgba(color).to_rgba(), color);
        assert_eq!(Xrgb8888::from_rgba(color).to_rgba().alpha, 255);
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
            P::write_opaque_gradient(&mut actual, color, step, [0.5; 4]);
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
