use blit::{
    Color, ImageData, ImageFormat, ImageId, PhysicalRect,
    widgets::{ImageRequest, ImageSampling, ImageTiling},
};

use crate::{Pixel, PixelBuffer, PremultipliedRgbaColor, Rgb8Pixel};

const FIXED_SHIFT: u32 = 16;
const FIXED_ONE: u64 = 1 << FIXED_SHIFT;

#[derive(Clone, Copy)]
pub struct Prepared {
    pub image: ImageId,
    pub bounds: PhysicalRect,
    display: PhysicalRect,
    source: PhysicalRect,
    texture_rect: PhysicalRect,
    stride_bytes: usize,
    bytes_per_pixel: usize,
    format: ImageFormat,
    colorize: Option<Color>,
    pub opacity: u8,
    sampling: ImageSampling,
    step_x: u64,
    step_y: u64,
    scale_x: f32,
    scale_y: f32,
    wrap_x: bool,
    wrap_y: bool,
}

#[derive(Clone, Copy)]
pub struct Patch {
    pub source: PhysicalRect,
    pub display: PhysicalRect,
    pub bounds: PhysicalRect,
    pub horizontal_tiling: ImageTiling,
    pub vertical_tiling: ImageTiling,
}

impl Prepared {
    pub fn new(
        request: &ImageRequest,
        texture: &ImageData,
        patch: Patch,
        scale_factor: f32,
    ) -> Option<Self> {
        let Patch {
            source,
            display,
            bounds,
            horizontal_tiling,
            vertical_tiling,
        } = patch;
        if source.width <= 0
            || source.height <= 0
            || display.width <= 0
            || display.height <= 0
            || bounds.width <= 0
            || bounds.height <= 0
            || request.opacity <= 0.0
        {
            return None;
        }
        let (step_x, scale_x, wrap_x) =
            axis(source.width, display.width, horizontal_tiling, scale_factor);
        let (step_y, scale_y, wrap_y) =
            axis(source.height, display.height, vertical_tiling, scale_factor);
        Some(Self {
            image: request.image,
            bounds,
            display,
            source,
            texture_rect: texture.texture_rect,
            stride_bytes: texture.stride_bytes,
            bytes_per_pixel: texture.format.bytes_per_pixel(),
            format: texture.format,
            colorize: request.colorize,
            opacity: (request.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            sampling: request.sampling,
            step_x,
            step_y,
            scale_x,
            scale_y,
            wrap_x,
            wrap_y,
        })
    }

    pub fn is_opaque(&self, pixels_opaque: bool) -> bool {
        pixels_opaque
            && self.opacity == 255
            && self.source.intersection(self.texture_rect) == Some(self.source)
            && match self.colorize {
                Some(color) => color.alpha == 255,
                None => !matches!(self.format, ImageFormat::Alpha8(color) if color.alpha != 255),
            }
    }

    pub fn draw<B: PixelBuffer>(&self, buffer: &mut B, texture: &ImageData, clip: PhysicalRect) {
        let screen = PhysicalRect {
            x: buffer.x_offset() as i32,
            y: 0,
            width: buffer.width() as i32,
            height: buffer.height() as i32,
        };
        let pixels = texture.pixels.bytes();
        let Some(clipped) = self
            .bounds
            .intersection(clip)
            .and_then(|area| area.intersection(screen))
        else {
            return;
        };
        for y in clipped.y..clipped.y + clipped.height {
            let row = buffer.line_mut(y as usize);
            match self.sampling {
                ImageSampling::Nearest => self.draw_nearest(row, pixels, clipped, screen.x, y),
                ImageSampling::Bilinear => self.draw_bilinear(row, pixels, clipped, screen.x, y),
            }
        }
    }

    fn draw_nearest<P: Pixel>(
        &self,
        row: &mut [P],
        pixels: &[u8],
        clipped: PhysicalRect,
        screen_x: i32,
        y: i32,
    ) {
        let source_y = self.source_y(y);
        let texture_x = self.texture_rect.x as usize;
        let texture_y = self.texture_rect.y as usize;
        if source_y < texture_y || source_y >= texture_y + self.texture_rect.height as usize {
            return;
        }
        let source_row = (source_y - texture_y) * self.stride_bytes;
        let texture_right = texture_x + self.texture_rect.width as usize;
        if self.colorize.is_some() && !matches!(self.format, ImageFormat::Alpha8(_)) {
            self.for_each_nearest_x(clipped, screen_x, |destination, source_x| {
                blend(
                    &mut row[destination],
                    self.source_pixel(pixels, source_x, source_y),
                );
            });
            return;
        }
        if let ImageFormat::Alpha8(color) = self.format
            && self.source.width == 1
        {
            let source_x = self.source.x as usize;
            if source_x < texture_x || source_x >= texture_right {
                return;
            }
            let source = source_row + source_x - texture_x;
            let alpha = (pixels[source] as u16 * self.opacity as u16 / 255) as u8;
            let start = (clipped.x - screen_x) as usize;
            P::blend_slice(
                &mut row[start..start + clipped.width as usize],
                PremultipliedRgbaColor::new(self.colorize.unwrap_or(color), alpha),
            );
            return;
        }
        if self.step_x == FIXED_ONE && self.draw_spans(row, pixels, clipped, screen_x, source_y) {
            return;
        }
        match self.format {
            ImageFormat::Rgb8 if self.opacity == 255 => {
                self.for_each_nearest_x(clipped, screen_x, |destination, source_x| {
                    if source_x >= texture_x && source_x < texture_right {
                        let source = source_row + (source_x - texture_x) * 3;
                        row[destination] =
                            P::from_rgb(pixels[source], pixels[source + 1], pixels[source + 2]);
                    }
                });
            }
            ImageFormat::Rgb8 => {
                self.for_each_nearest_x(clipped, screen_x, |destination, source_x| {
                    if source_x >= texture_x && source_x < texture_right {
                        let source = source_row + (source_x - texture_x) * 3;
                        blend(
                            &mut row[destination],
                            PremultipliedRgbaColor::new(
                                Color::from_rgba8(
                                    pixels[source],
                                    pixels[source + 1],
                                    pixels[source + 2],
                                    255,
                                ),
                                self.opacity,
                            ),
                        );
                    }
                });
            }
            ImageFormat::Rgba8 => {
                self.for_each_nearest_x(clipped, screen_x, |destination, source_x| {
                    if source_x >= texture_x && source_x < texture_right {
                        let source = source_row + (source_x - texture_x) * 4;
                        let alpha = (pixels[source + 3] as u16 * self.opacity as u16 / 255) as u8;
                        blend(
                            &mut row[destination],
                            PremultipliedRgbaColor {
                                red: (pixels[source] as u16 * alpha as u16 / 255) as u8,
                                green: (pixels[source + 1] as u16 * alpha as u16 / 255) as u8,
                                blue: (pixels[source + 2] as u16 * alpha as u16 / 255) as u8,
                                alpha,
                            },
                        );
                    }
                });
            }
            ImageFormat::Rgba8Premultiplied if self.opacity == 255 => {
                self.for_each_nearest_x(clipped, screen_x, |destination, source_x| {
                    if source_x >= texture_x && source_x < texture_right {
                        let source = source_row + (source_x - texture_x) * 4;
                        blend(
                            &mut row[destination],
                            PremultipliedRgbaColor {
                                red: pixels[source],
                                green: pixels[source + 1],
                                blue: pixels[source + 2],
                                alpha: pixels[source + 3],
                            },
                        );
                    }
                });
            }
            ImageFormat::Rgba8Premultiplied => {
                self.for_each_nearest_x(clipped, screen_x, |destination, source_x| {
                    if source_x >= texture_x && source_x < texture_right {
                        let source = source_row + (source_x - texture_x) * 4;
                        blend(
                            &mut row[destination],
                            PremultipliedRgbaColor {
                                red: (pixels[source] as u16 * self.opacity as u16 / 255) as u8,
                                green: (pixels[source + 1] as u16 * self.opacity as u16 / 255)
                                    as u8,
                                blue: (pixels[source + 2] as u16 * self.opacity as u16 / 255) as u8,
                                alpha: (pixels[source + 3] as u16 * self.opacity as u16 / 255)
                                    as u8,
                            },
                        );
                    }
                });
            }
            ImageFormat::Alpha8(color) => {
                let color = self.colorize.unwrap_or(color);
                self.for_each_nearest_x(clipped, screen_x, |destination, source_x| {
                    if source_x >= texture_x && source_x < texture_right {
                        let source = source_row + source_x - texture_x;
                        blend(
                            &mut row[destination],
                            PremultipliedRgbaColor::new(
                                color,
                                (pixels[source] as u16 * self.opacity as u16 / 255) as u8,
                            ),
                        );
                    }
                });
            }
        }
    }

    fn draw_spans<P: Pixel>(
        &self,
        row: &mut [P],
        pixels: &[u8],
        clipped: PhysicalRect,
        screen_x: i32,
        source_y: usize,
    ) -> bool {
        if !matches!(
            self.format,
            ImageFormat::Rgb8 | ImageFormat::Rgba8Premultiplied | ImageFormat::Alpha8(_)
        ) || self.format == ImageFormat::Rgb8 && self.opacity != 255
        {
            return false;
        }
        let texture_x = self.texture_rect.x as usize;
        let texture_y = self.texture_rect.y as usize;
        let texture_right = texture_x + self.texture_rect.width as usize;
        let source_row = (source_y - texture_y) * self.stride_bytes;
        let mut destination_x = clipped.x;
        let destination_end = clipped.x + clipped.width;
        let mut source = self.source_fixed_x(destination_x);
        while destination_x < destination_end {
            let source_x = self.source.x as usize + (source >> FIXED_SHIFT) as usize;
            let source_end = (self.source.x + self.source.width) as usize;
            let len =
                (destination_end - destination_x).min((source_end - source_x) as i32) as usize;
            if source_x < texture_x || source_x + len > texture_right {
                return false;
            }
            let destination = (destination_x - screen_x) as usize;
            let destination = &mut row[destination..destination + len];
            let source_offset = source_row + (source_x - texture_x) * self.bytes_per_pixel;
            match self.format {
                ImageFormat::Rgb8 => {
                    let bytes = &pixels[source_offset..source_offset + len * 3];
                    let (prefix, source, suffix) = unsafe { bytes.align_to::<Rgb8Pixel>() };
                    assert!(prefix.is_empty() && suffix.is_empty());
                    P::blend_texture_slice_rgb(destination, source);
                }
                ImageFormat::Rgba8Premultiplied => {
                    let bytes = &pixels[source_offset..source_offset + len * 4];
                    let (prefix, source, suffix) =
                        unsafe { bytes.align_to::<PremultipliedRgbaColor>() };
                    assert!(prefix.is_empty() && suffix.is_empty());
                    P::blend_texture_slice_rgba(destination, source, self.opacity);
                }
                ImageFormat::Alpha8(color) => {
                    let alpha = &pixels[source_offset..source_offset + len];
                    let color = self.colorize.unwrap_or(color);
                    let color = Color::from_rgba8(
                        color.red,
                        color.green,
                        color.blue,
                        (color.alpha as u16 * self.opacity as u16 / 255) as u8,
                    );
                    P::blend_texture_slice_alpha(destination, color, alpha);
                }
                _ => unreachable!(),
            }
            destination_x += len as i32;
            source += (len as u64) << FIXED_SHIFT;
            if self.wrap_x && source >= (self.source.width as u64) << FIXED_SHIFT {
                source %= (self.source.width as u64) << FIXED_SHIFT;
            }
        }
        true
    }

    #[inline(always)]
    fn for_each_nearest_x(
        &self,
        clipped: PhysicalRect,
        screen_x: i32,
        mut process: impl FnMut(usize, usize),
    ) {
        let mut source = self.source_fixed_x(clipped.x);
        let source_span = (self.source.width as u64) << FIXED_SHIFT;
        for x in clipped.x..clipped.x + clipped.width {
            let source_x = self.source.x as usize
                + (source >> FIXED_SHIFT).min(self.source.width as u64 - 1) as usize;
            process((x - screen_x) as usize, source_x);
            source += self.step_x;
            if self.wrap_x && source >= source_span {
                source %= source_span;
            }
        }
    }

    fn draw_bilinear<P: Pixel>(
        &self,
        row: &mut [P],
        pixels: &[u8],
        clipped: PhysicalRect,
        screen_x: i32,
        y: i32,
    ) {
        let source_y = self.source_float_y(y);
        let top = source_y.floor() as usize;
        let bottom = if top + 1 < self.source.height as usize {
            top + 1
        } else if self.wrap_y {
            0
        } else {
            top
        };
        let vertical = source_y - top as f32;
        for x in clipped.x..clipped.x + clipped.width {
            let source_x = self.source_float_x(x);
            let left = source_x.floor() as usize;
            let right = if left + 1 < self.source.width as usize {
                left + 1
            } else if self.wrap_x {
                0
            } else {
                left
            };
            let horizontal = source_x - left as f32;
            let top_left = self.source_pixel(
                pixels,
                self.source.x as usize + left,
                self.source.y as usize + top,
            );
            let top_right = self.source_pixel(
                pixels,
                self.source.x as usize + right,
                self.source.y as usize + top,
            );
            let bottom_left = self.source_pixel(
                pixels,
                self.source.x as usize + left,
                self.source.y as usize + bottom,
            );
            let bottom_right = self.source_pixel(
                pixels,
                self.source.x as usize + right,
                self.source.y as usize + bottom,
            );
            let interpolate = |top_left: u8, top_right: u8, bottom_left: u8, bottom_right: u8| {
                let top = top_left as f32 + (top_right as f32 - top_left as f32) * horizontal;
                let bottom =
                    bottom_left as f32 + (bottom_right as f32 - bottom_left as f32) * horizontal;
                (top + (bottom - top) * vertical).round() as u8
            };
            blend(
                &mut row[(x - screen_x) as usize],
                PremultipliedRgbaColor {
                    red: interpolate(
                        top_left.red,
                        top_right.red,
                        bottom_left.red,
                        bottom_right.red,
                    ),
                    green: interpolate(
                        top_left.green,
                        top_right.green,
                        bottom_left.green,
                        bottom_right.green,
                    ),
                    blue: interpolate(
                        top_left.blue,
                        top_right.blue,
                        bottom_left.blue,
                        bottom_right.blue,
                    ),
                    alpha: interpolate(
                        top_left.alpha,
                        top_right.alpha,
                        bottom_left.alpha,
                        bottom_right.alpha,
                    ),
                },
            );
        }
    }

    fn source_y(&self, y: i32) -> usize {
        let mut source = (y - self.display.y) as u64 * self.step_y;
        let source_span = (self.source.height as u64) << FIXED_SHIFT;
        if self.wrap_y {
            source %= source_span;
        }
        self.source.y as usize + (source >> FIXED_SHIFT).min(self.source.height as u64 - 1) as usize
    }

    fn source_fixed_x(&self, x: i32) -> u64 {
        let source = (x - self.display.x) as u64 * self.step_x;
        if self.wrap_x {
            source % ((self.source.width as u64) << FIXED_SHIFT)
        } else {
            source
        }
    }

    fn source_float_x(&self, x: i32) -> f32 {
        let source = ((x - self.display.x) as f32 + 0.5) * self.scale_x - 0.5;
        if self.wrap_x {
            source.rem_euclid(self.source.width as f32)
        } else {
            source.clamp(0.0, self.source.width as f32 - 1.0)
        }
    }

    fn source_float_y(&self, y: i32) -> f32 {
        let source = ((y - self.display.y) as f32 + 0.5) * self.scale_y - 0.5;
        if self.wrap_y {
            source.rem_euclid(self.source.height as f32)
        } else {
            source.clamp(0.0, self.source.height as f32 - 1.0)
        }
    }

    fn source_pixel(&self, pixels: &[u8], x: usize, y: usize) -> PremultipliedRgbaColor {
        let texture_x = self.texture_rect.x as usize;
        let texture_y = self.texture_rect.y as usize;
        if x < texture_x
            || y < texture_y
            || x >= texture_x + self.texture_rect.width as usize
            || y >= texture_y + self.texture_rect.height as usize
        {
            return PremultipliedRgbaColor::default();
        }
        let offset = (y - texture_y) * self.stride_bytes + (x - texture_x) * self.bytes_per_pixel;
        let pixel = match self.format {
            ImageFormat::Rgba8Premultiplied if self.opacity == 255 => PremultipliedRgbaColor {
                red: pixels[offset],
                green: pixels[offset + 1],
                blue: pixels[offset + 2],
                alpha: pixels[offset + 3],
            },
            ImageFormat::Rgba8Premultiplied => PremultipliedRgbaColor {
                red: (pixels[offset] as u16 * self.opacity as u16 / 255) as u8,
                green: (pixels[offset + 1] as u16 * self.opacity as u16 / 255) as u8,
                blue: (pixels[offset + 2] as u16 * self.opacity as u16 / 255) as u8,
                alpha: (pixels[offset + 3] as u16 * self.opacity as u16 / 255) as u8,
            },
            ImageFormat::Rgb8 => PremultipliedRgbaColor::new(
                Color::from_rgba8(pixels[offset], pixels[offset + 1], pixels[offset + 2], 255),
                self.opacity,
            ),
            ImageFormat::Rgba8 => PremultipliedRgbaColor::new(
                Color::from_rgba8(
                    pixels[offset],
                    pixels[offset + 1],
                    pixels[offset + 2],
                    pixels[offset + 3],
                ),
                self.opacity,
            ),
            ImageFormat::Alpha8(color) => PremultipliedRgbaColor::new(
                color,
                (pixels[offset] as u16 * self.opacity as u16 / 255) as u8,
            ),
        };
        self.colorize.map_or(pixel, |color| {
            PremultipliedRgbaColor::new(color, pixel.alpha)
        })
    }
}

fn axis(source: i32, target: i32, tiling: ImageTiling, scale_factor: f32) -> (u64, f32, bool) {
    match tiling {
        ImageTiling::None => (
            ((source as u64) << FIXED_SHIFT) / target as u64,
            source as f32 / target as f32,
            false,
        ),
        ImageTiling::Repeat => {
            let tile = (source as f32 * scale_factor).round().max(1.0) as u64;
            (
                ((source as u64) << FIXED_SHIFT) / tile,
                source as f32 / tile as f32,
                true,
            )
        }
        ImageTiling::Round => {
            let native = (source as f32 * scale_factor).max(1.0);
            let count = (target as f32 / native).round().max(1.0) as u64;
            (
                ((source as u64 * count) << FIXED_SHIFT) / target as u64,
                source as f32 * count as f32 / target as f32,
                true,
            )
        }
    }
}

#[inline(always)]
fn blend<P: Pixel>(pixel: &mut P, color: PremultipliedRgbaColor) {
    if color.alpha == 255 {
        *pixel = P::from_rgb(color.red, color.green, color.blue);
    } else if color.alpha != 0 {
        pixel.blend(color);
    }
}
