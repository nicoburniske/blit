use bullseye::{
    Color, ImageData, ImageFormat, PhysicalRect,
    widgets::{ImageFit, ImageRequest, ImageSampling},
};

use crate::{Pixel, PixelBuffer, PremultipliedRgbaColor};

pub fn draw<B: PixelBuffer>(
    buffer: &mut B,
    request: &ImageRequest,
    texture: &ImageData,
    clips: &[PhysicalRect],
    scale_factor: f32,
) {
    let geometry = request.area.to_physical(scale_factor);
    let pixels = texture.pixels.bytes();
    let width = texture.size.width as usize;
    let height = texture.size.height as usize;
    let bytes_per_pixel = texture.format.bytes_per_pixel();
    if geometry.width <= 0
        || geometry.height <= 0
        || width == 0
        || height == 0
        || request.opacity <= 0.0
    {
        return;
    }

    let display = match request.fit {
        ImageFit::Fill => geometry,
        ImageFit::Contain | ImageFit::Cover => {
            let horizontal = geometry.width as f32 / width as f32;
            let vertical = geometry.height as f32 / height as f32;
            let scale = if request.fit == ImageFit::Contain {
                horizontal.min(vertical)
            } else {
                horizontal.max(vertical)
            };
            let width = (width as f32 * scale).round().max(1.0) as i32;
            let height = (height as f32 * scale).round().max(1.0) as i32;
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
    let screen = PhysicalRect {
        x: 0,
        y: 0,
        width: buffer.width() as i32,
        height: buffer.height() as i32,
    };
    let opacity = (request.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    let source_pixel = |x: usize, y: usize| {
        let texture_x = texture.texture_rect.x as usize;
        let texture_y = texture.texture_rect.y as usize;
        if x < texture_x
            || y < texture_y
            || x >= texture_x + texture.texture_rect.width as usize
            || y >= texture_y + texture.texture_rect.height as usize
        {
            return PremultipliedRgbaColor::default();
        }
        let offset = (y - texture_y) * texture.stride_bytes + (x - texture_x) * bytes_per_pixel;
        match texture.format {
            ImageFormat::Rgba8Premultiplied => PremultipliedRgbaColor {
                red: (pixels[offset] as u16 * opacity as u16 / 255) as u8,
                green: (pixels[offset + 1] as u16 * opacity as u16 / 255) as u8,
                blue: (pixels[offset + 2] as u16 * opacity as u16 / 255) as u8,
                alpha: (pixels[offset + 3] as u16 * opacity as u16 / 255) as u8,
            },
            ImageFormat::Rgb8 => PremultipliedRgbaColor::new(
                Color::from_rgba8(pixels[offset], pixels[offset + 1], pixels[offset + 2], 255),
                opacity,
            ),
            ImageFormat::Rgba8 => PremultipliedRgbaColor::new(
                Color::from_rgba8(
                    pixels[offset],
                    pixels[offset + 1],
                    pixels[offset + 2],
                    pixels[offset + 3],
                ),
                opacity,
            ),
            ImageFormat::Alpha8(color) => PremultipliedRgbaColor::new(
                color,
                (pixels[offset] as u16 * opacity as u16 / 255) as u8,
            ),
        }
    };

    for clip in clips {
        let Some(clipped) = bounds
            .intersection(*clip)
            .and_then(|area| area.intersection(screen))
        else {
            continue;
        };
        for y in clipped.y..clipped.y + clipped.height {
            let row = buffer.line_mut(y as usize);
            for x in clipped.x..clipped.x + clipped.width {
                let color = match request.sampling {
                    ImageSampling::Nearest => {
                        let source_x =
                            ((x - display.x) as i64 * width as i64 / display.width as i64)
                                .clamp(0, width as i64 - 1) as usize;
                        let source_y =
                            ((y - display.y) as i64 * height as i64 / display.height as i64)
                                .clamp(0, height as i64 - 1) as usize;
                        source_pixel(source_x, source_y)
                    }
                    ImageSampling::Bilinear => {
                        let source_x = (((x - display.x) as f32 + 0.5) * width as f32
                            / display.width as f32
                            - 0.5)
                            .clamp(0.0, width as f32 - 1.0);
                        let source_y = (((y - display.y) as f32 + 0.5) * height as f32
                            / display.height as f32
                            - 0.5)
                            .clamp(0.0, height as f32 - 1.0);
                        let left = source_x.floor() as usize;
                        let top = source_y.floor() as usize;
                        let right = (left + 1).min(width - 1);
                        let bottom = (top + 1).min(height - 1);
                        let horizontal = source_x - left as f32;
                        let vertical = source_y - top as f32;
                        let top_left = source_pixel(left, top);
                        let top_right = source_pixel(right, top);
                        let bottom_left = source_pixel(left, bottom);
                        let bottom_right = source_pixel(right, bottom);
                        let interpolate =
                            |top_left: u8, top_right: u8, bottom_left: u8, bottom_right: u8| {
                                let top = top_left as f32
                                    + (top_right as f32 - top_left as f32) * horizontal;
                                let bottom = bottom_left as f32
                                    + (bottom_right as f32 - bottom_left as f32) * horizontal;
                                (top + (bottom - top) * vertical).round() as u8
                            };
                        PremultipliedRgbaColor {
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
                        }
                    }
                };
                row[x as usize].blend(color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bullseye::{
        ImageData, ImageFormat, ImageId, ImagePixels, LogicalRect,
        widgets::{ImageFit, ImageRequest, ImageSampling},
    };

    use super::*;
    use crate::VecBuffer;

    #[test]
    fn nearest_scaled_image_respects_clip() {
        static PIXELS: [u8; 16] = [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgba8, 2, 2);
        let request = ImageRequest {
            image: ImageId(0),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 4.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
        };
        let mut buffer = VecBuffer::<u32>::new(4, 4);

        draw(
            &mut buffer,
            &request,
            &texture,
            &[PhysicalRect {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            }],
            1.0,
        );

        assert_eq!(buffer.pixels()[0], 0);
        assert_eq!(buffer.pixels()[5], 0xff0000);
        assert_eq!(buffer.pixels()[6], 0x00ff00);
        assert_eq!(buffer.pixels()[9], 0x0000ff);
        assert_eq!(buffer.pixels()[10], 0xffffff);
        assert_eq!(buffer.pixels()[15], 0);
    }
}
