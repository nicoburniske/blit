use bullseye::{
    Color, ImageData, ImageFormat, ImageId, PhysicalRect,
    widgets::{ImageFit, ImageRequest, ImageSampling},
};

use crate::{Pixel, PixelBuffer, PremultipliedRgbaColor, Rgb8Pixel};

const FIXED_SHIFT: u32 = 16;

#[derive(Clone, Copy)]
pub struct Prepared {
    pub image: ImageId,
    display: PhysicalRect,
    bounds: PhysicalRect,
    texture_rect: PhysicalRect,
    source_width: usize,
    source_height: usize,
    stride_bytes: usize,
    bytes_per_pixel: usize,
    format: ImageFormat,
    opacity: u8,
    sampling: ImageSampling,
    scale_x: f32,
    scale_y: f32,
    step_x: u64,
    step_y: u64,
}

impl Prepared {
    pub fn new(request: &ImageRequest, texture: &ImageData, scale_factor: f32) -> Option<Self> {
        let geometry = request.area.to_physical(scale_factor);
        let source_width = texture.size.width as usize;
        let source_height = texture.size.height as usize;
        if geometry.width <= 0
            || geometry.height <= 0
            || source_width == 0
            || source_height == 0
            || request.opacity <= 0.0
        {
            return None;
        }

        let display = match request.fit {
            ImageFit::Fill => geometry,
            ImageFit::Contain | ImageFit::Cover => {
                let horizontal = geometry.width as f32 / source_width as f32;
                let vertical = geometry.height as f32 / source_height as f32;
                let scale = if request.fit == ImageFit::Contain {
                    horizontal.min(vertical)
                } else {
                    horizontal.max(vertical)
                };
                let width = (source_width as f32 * scale).round().max(1.0) as i32;
                let height = (source_height as f32 * scale).round().max(1.0) as i32;
                PhysicalRect {
                    x: geometry.x + (geometry.width - width) / 2,
                    y: geometry.y + (geometry.height - height) / 2,
                    width,
                    height,
                }
            }
        };
        let bounds = if request.fit == ImageFit::Cover {
            display.intersection(geometry).unwrap_or_default()
        } else {
            display
        };
        Some(Self {
            image: request.image,
            display,
            bounds,
            texture_rect: texture.texture_rect,
            source_width,
            source_height,
            stride_bytes: texture.stride_bytes,
            bytes_per_pixel: texture.format.bytes_per_pixel(),
            format: texture.format,
            opacity: (request.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            sampling: request.sampling,
            scale_x: source_width as f32 / display.width as f32,
            scale_y: source_height as f32 / display.height as f32,
            step_x: ((source_width as u64) << FIXED_SHIFT) / display.width as u64,
            step_y: ((source_height as u64) << FIXED_SHIFT) / display.height as u64,
        })
    }

    pub fn draw<B: PixelBuffer>(
        &self,
        buffer: &mut B,
        texture: &ImageData,
        clips: &[PhysicalRect],
    ) {
        let screen = PhysicalRect {
            x: buffer.x_offset() as i32,
            y: 0,
            width: buffer.width() as i32,
            height: buffer.height() as i32,
        };
        let pixels = texture.pixels.bytes();
        for clip in clips {
            let Some(clipped) = self
                .bounds
                .intersection(*clip)
                .and_then(|area| area.intersection(screen))
            else {
                continue;
            };
            for y in clipped.y..clipped.y + clipped.height {
                let row = buffer.line_mut(y as usize);
                match self.sampling {
                    ImageSampling::Nearest => self.draw_nearest(row, pixels, clipped, screen.x, y),
                    ImageSampling::Bilinear => {
                        let source_y = (((y - self.display.y) as f32 + 0.5) * self.scale_y - 0.5)
                            .clamp(0.0, self.source_height as f32 - 1.0);
                        let top = source_y.floor() as usize;
                        let bottom = (top + 1).min(self.source_height - 1);
                        let vertical = source_y - top as f32;
                        for x in clipped.x..clipped.x + clipped.width {
                            let source_x = (((x - self.display.x) as f32 + 0.5) * self.scale_x
                                - 0.5)
                                .clamp(0.0, self.source_width as f32 - 1.0);
                            let left = source_x.floor() as usize;
                            let right = (left + 1).min(self.source_width - 1);
                            let horizontal = source_x - left as f32;
                            let top_left = self.source_pixel(pixels, left, top);
                            let top_right = self.source_pixel(pixels, right, top);
                            let bottom_left = self.source_pixel(pixels, left, bottom);
                            let bottom_right = self.source_pixel(pixels, right, bottom);
                            let interpolate =
                                |top_left: u8, top_right: u8, bottom_left: u8, bottom_right: u8| {
                                    let top = top_left as f32
                                        + (top_right as f32 - top_left as f32) * horizontal;
                                    let bottom = bottom_left as f32
                                        + (bottom_right as f32 - bottom_left as f32) * horizontal;
                                    (top + (bottom - top) * vertical).round() as u8
                                };
                            row[(x - screen.x) as usize].blend(PremultipliedRgbaColor {
                                alpha: interpolate(
                                    top_left.alpha,
                                    top_right.alpha,
                                    bottom_left.alpha,
                                    bottom_right.alpha,
                                ),
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
                            });
                        }
                    }
                }
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
        let source_y = (((y - self.display.y) as u64 * self.step_y) >> FIXED_SHIFT)
            .min(self.source_height as u64 - 1) as usize;
        let texture_x = self.texture_rect.x as usize;
        let texture_y = self.texture_rect.y as usize;
        if source_y < texture_y || source_y >= texture_y + self.texture_rect.height as usize {
            return;
        }
        let source_row = (source_y - texture_y) * self.stride_bytes;
        let texture_right = texture_x + self.texture_rect.width as usize;
        if self.step_x == 1 << FIXED_SHIFT {
            let source_x =
                (((clipped.x - self.display.x) as u64 * self.step_x) >> FIXED_SHIFT) as usize;
            let len = clipped.width as usize;
            if source_x >= texture_x && source_x + len <= texture_right {
                let destination = (clipped.x - screen_x) as usize;
                let destination = &mut row[destination..destination + len];
                let source = source_row + (source_x - texture_x) * self.bytes_per_pixel;
                match self.format {
                    ImageFormat::Rgb8 if self.opacity == 255 => {
                        let bytes = &pixels[source..source + len * 3];
                        let (prefix, source, suffix) = unsafe { bytes.align_to::<Rgb8Pixel>() };
                        assert!(prefix.is_empty() && suffix.is_empty());
                        P::blend_texture_slice_rgb(destination, source);
                        return;
                    }
                    ImageFormat::Rgba8Premultiplied if self.opacity == 255 => {
                        let bytes = &pixels[source..source + len * 4];
                        let (prefix, source, suffix) =
                            unsafe { bytes.align_to::<PremultipliedRgbaColor>() };
                        assert!(prefix.is_empty() && suffix.is_empty());
                        P::blend_texture_slice_rgba(destination, source);
                        return;
                    }
                    ImageFormat::Alpha8(color) => {
                        let alpha = &pixels[source..source + len];
                        let color = Color::from_rgba8(
                            color.red,
                            color.green,
                            color.blue,
                            (color.alpha as u16 * self.opacity as u16 / 255) as u8,
                        );
                        P::blend_texture_slice_alpha(destination, color, alpha);
                        return;
                    }
                    _ => {}
                }
            }
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

    #[inline(always)]
    fn for_each_nearest_x(
        &self,
        clipped: PhysicalRect,
        screen_x: i32,
        mut process: impl FnMut(usize, usize),
    ) {
        let mut source = (clipped.x - self.display.x) as u64 * self.step_x;
        for x in clipped.x..clipped.x + clipped.width {
            let source_x = (source >> FIXED_SHIFT).min(self.source_width as u64 - 1) as usize;
            process((x - screen_x) as usize, source_x);
            source += self.step_x;
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
        match self.format {
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
