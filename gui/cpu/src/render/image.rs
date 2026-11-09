use blit::PhysicalRect;
use blit_gui::{
    image::{ImageData, ImageFormat, ImageHandle, ImagePatch, ImageSampling, ImageTiling, prepare_image_patches},
    paint::ImageRequest,
};

use super::image_patch::{AlphaRows, Prepared};

pub struct StoredImage {
    pub handle: ImageHandle,
    pub data: ImageData,
    pub alpha_rows: AlphaRows,
    pub has_opaque_spans: bool,
    pub opaque: bool,
}

impl StoredImage {
    pub fn new(handle: ImageHandle, data: ImageData) -> Self {
        let width = data.texture_rect.width as usize;
        let height = data.texture_rect.height as usize;
        let bytes = data.pixels.bytes();
        let mut has_opaque_spans = false;
        let rgba_opaque = || {
            (0..height).all(|line| {
                bytes[line * data.stride_bytes..][..width * 4]
                    .chunks_exact(4)
                    .all(|pixel| pixel[3] == 255)
            })
        };
        let (alpha_rows, opaque) = match data.format {
            ImageFormat::Rgb8 | ImageFormat::Luma8 => (AlphaRows::default(), true),
            ImageFormat::Rgba8 => (AlphaRows::default(), rgba_opaque()),
            ImageFormat::Rgba8Premultiplied if rgba_opaque() => (AlphaRows::default(), true),
            ImageFormat::Rgba8Premultiplied if width > u16::MAX as usize => (AlphaRows::default(), false),
            ImageFormat::Rgba8Premultiplied => {
                let mut rows = Vec::with_capacity(height * 4);
                for y in 0..height {
                    let row = &bytes[y * data.stride_bytes..][..width * 4];
                    let mut visible_start = width;
                    let mut visible_end = 0;
                    let mut run_start = 0;
                    let mut opaque_start = 0;
                    let mut opaque_end = 0;
                    for (x, alpha) in row.chunks_exact(4).map(|pixel| pixel[3]).chain([0]).enumerate() {
                        if alpha != 0 {
                            visible_start = visible_start.min(x);
                            visible_end = x + 1;
                        }
                        if alpha == 255 {
                            continue;
                        }
                        if x - run_start > opaque_end - opaque_start {
                            opaque_start = run_start;
                            opaque_end = x;
                        }
                        run_start = x + 1;
                    }
                    visible_start = visible_start.min(visible_end);
                    has_opaque_spans |= opaque_start < opaque_end;
                    rows.extend([
                        visible_start as u16,
                        visible_end as u16,
                        opaque_start as u16,
                        opaque_end as u16,
                    ]);
                }
                (AlphaRows(rows.into_boxed_slice()), false)
            }
            ImageFormat::Alpha8(_)
                if (0..height).all(|line| {
                    bytes[line * data.stride_bytes..][..width]
                        .iter()
                        .all(|alpha| *alpha == 255)
                }) =>
            {
                (AlphaRows::default(), true)
            }
            ImageFormat::Alpha8(_) if width > u16::MAX as usize => (AlphaRows::default(), false),
            ImageFormat::Alpha8(_) => {
                let mut rows = Vec::with_capacity(height * 2);
                for y in 0..height {
                    let row = &bytes[y * data.stride_bytes..][..width];
                    let mut visible_start = width;
                    let mut visible_end = 0;
                    for (x, alpha) in row.iter().enumerate() {
                        if *alpha != 0 {
                            visible_start = visible_start.min(x);
                            visible_end = x + 1;
                        }
                    }
                    visible_start = visible_start.min(visible_end);
                    rows.extend([visible_start as u16, visible_end as u16]);
                }
                (AlphaRows(rows.into_boxed_slice()), false)
            }
        };
        Self {
            handle,
            data,
            alpha_rows,
            has_opaque_spans,
            opaque,
        }
    }
}

pub fn prepare(
    request: &ImageRequest,
    texture: &ImageData,
    clip: PhysicalRect,
    scale_factor: f32,
    mut emit: impl FnMut(Prepared, PhysicalRect),
) {
    prepare_image_patches(request, texture.size, scale_factor, |patch| {
        if let Some(prepared) = Prepared::new(request, patch, scale_factor)
            && let Some(bounds) = patch.bounds.intersection(clip)
        {
            emit(prepared, bounds);
        }
    });
}

pub fn coverage(
    request: &ImageRequest,
    patch: ImagePatch,
    texture: &ImageData,
    pixels_opaque: bool,
    has_opaque_spans: bool,
) -> (bool, bool) {
    let full_opacity = (request.opacity.clamp(0.0, 1.0) * 255.0).round() as u8 == 255;
    let valid_source = patch.source.intersection(texture.texture_rect) == Some(patch.source);
    let opaque = pixels_opaque
        && full_opacity
        && valid_source
        && request.colorize.is_none_or(|color| color.alpha == 255)
        && !matches!(texture.format, ImageFormat::Alpha8(color) if color.alpha != 255);
    let partial = has_opaque_spans
        && full_opacity
        && valid_source
        && request.colorize.is_none()
        && request.sampling == ImageSampling::Nearest
        && patch.source.width == patch.display.width
        && patch.horizontal_tiling == ImageTiling::None
        && patch.vertical_tiling == ImageTiling::None;
    (opaque, partial)
}

#[cfg(test)]
mod tests {
    use blit::{LogicalRect, PhysicalRect};
    use blit_gui::{
        color::Color,
        image::{
            ImageFit, ImageFormat, ImageId, ImagePixels, ImageSampling, ImageTiling, NineSlice, prepare_image_patches,
        },
    };

    use super::*;
    use crate::{PixelBuffer, VecBuffer, Xrgb8888, render::image_patch::AlphaRows};

    fn draw<B: PixelBuffer>(
        buffer: &mut B,
        request: &ImageRequest,
        texture: &ImageData,
        clip: PhysicalRect,
        scale_factor: f32,
    ) {
        let alpha_rows = AlphaRows::default();
        prepare(request, texture, clip, scale_factor, |image, clip| {
            let screen = PhysicalRect {
                x: buffer.x_offset() as i32,
                y: 0,
                width: buffer.width() as i32,
                height: buffer.height() as i32,
            };
            if let Some(clip) = clip.intersection(screen) {
                for y in clip.y..clip.y + clip.height {
                    image.draw_line(buffer.line_mut(y as usize), texture, &alpha_rows, clip, screen.x, y);
                }
            }
        });
    }

    #[test]
    fn nearest_scaled_image_respects_clip() {
        static PIXELS: [u8; 16] = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgba8, 2, 2);
        let request = ImageRequest {
            image: ImageId::default(),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 4.0,
                height: 4.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<Xrgb8888>::new(4, 4);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            },
            1.0,
        );

        assert_eq!(buffer.pixels()[0].raw(), 0);
        assert_eq!(buffer.pixels()[5].raw(), 0xff0000);
        assert_eq!(buffer.pixels()[6].raw(), 0x00ff00);
        assert_eq!(buffer.pixels()[9].raw(), 0x0000ff);
        assert_eq!(buffer.pixels()[10].raw(), 0xffffff);
        assert_eq!(buffer.pixels()[15].raw(), 0);
    }

    #[test]
    fn colorize_uses_rgba_alpha() {
        static PIXELS: [u8; 4] = [16, 8, 4, 128];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgba8Premultiplied, 1, 1);
        let request = ImageRequest {
            image: ImageId::default(),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: Some(Color::WHITE),
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<Xrgb8888>::new(1, 1);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            1.0,
        );

        assert_eq!(buffer.pixels()[0].raw(), 0x808080);

        static ALPHA: [u8; 2] = [255, 255];
        let texture = ImageData::new(
            ImagePixels::Static(&ALPHA),
            ImageFormat::Alpha8(Color::rgba(255, 255, 255, 128)),
            2,
            1,
        );
        let request = ImageRequest {
            area: LogicalRect::new(0.0, 0.0, 3.0, 1.0),
            colorize: Some(Color::rgb(255, 0, 0)),
            ..request
        };
        let mut buffer = VecBuffer::<Xrgb8888>::new(3, 1);
        let clip = PhysicalRect {
            x: 0,
            y: 0,
            width: 3,
            height: 1,
        };

        draw(&mut buffer, &request, &texture, clip, 1.0);

        assert_eq!(buffer.pixels(), [0x800000, 0x800000, 0x800000].map(Xrgb8888::from_raw));
        let mut opaque = None;
        prepare_image_patches(&request, texture.size, 1.0, |patch| {
            opaque = Some(coverage(&request, patch, &texture, true, false).0);
        });
        assert_eq!(opaque, Some(false));
    }

    #[test]
    fn bilinear_image_interpolates_source_pixels() {
        static PIXELS: [u8; 8] = [255, 0, 0, 255, 0, 0, 255, 255];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgba8, 2, 1);
        let request = ImageRequest {
            image: ImageId::default(),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 3.0,
                height: 1.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Bilinear,
            opacity: 1.0,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<Xrgb8888>::new(3, 1);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 0,
                y: 0,
                width: 3,
                height: 1,
            },
            1.0,
        );

        assert_eq!(buffer.pixels(), [0xff0000, 0x800080, 0x0000ff].map(Xrgb8888::from_raw));
    }

    #[test]
    fn unscaled_premultiplied_image_applies_opacity() {
        static PIXELS: [u8; 8] = [255, 0, 0, 255, 0, 128, 0, 128];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgba8Premultiplied, 2, 1);
        let request = ImageRequest {
            image: ImageId::default(),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 2.0,
                height: 1.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 0.5,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<Xrgb8888>::new(2, 1);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 0,
                y: 0,
                width: 2,
                height: 1,
            },
            1.0,
        );

        assert_eq!(buffer.pixels(), [0x800000, 0x004000].map(Xrgb8888::from_raw));
    }

    #[test]
    fn image_repeats_horizontally() {
        static PIXELS: [u8; 6] = [255, 0, 0, 0, 0, 255];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgb8, 2, 1);
        let request = ImageRequest {
            image: ImageId::default(),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 5.0,
                height: 1.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::Repeat,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<Xrgb8888>::new(5, 1);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 0,
                y: 0,
                width: 5,
                height: 1,
            },
            1.0,
        );

        assert_eq!(
            buffer.pixels(),
            [0xff0000, 0x0000ff, 0xff0000, 0x0000ff, 0xff0000].map(Xrgb8888::from_raw)
        );
    }

    #[test]
    fn round_tiling_fits_complete_tiles() {
        static PIXELS: [u8; 9] = [255, 0, 0, 0, 255, 0, 0, 0, 255];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgb8, 3, 1);
        let request = ImageRequest {
            image: ImageId::default(),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 8.0,
                height: 1.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::Round,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<Xrgb8888>::new(8, 1);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 0,
                y: 0,
                width: 8,
                height: 1,
            },
            1.0,
        );

        assert_eq!(
            buffer.pixels(),
            [
                0xff0000, 0x00ff00, 0x0000ff, 0xff0000, 0x00ff00, 0x0000ff, 0xff0000, 0x00ff00
            ]
            .map(Xrgb8888::from_raw)
        );
    }

    #[test]
    fn nine_slice_preserves_corners() {
        static PIXELS: [u8; 27] = [
            255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0, 255, 0, 255, 0, 255, 255, 128, 0, 0, 0, 128, 0, 0, 0, 128,
        ];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgb8, 3, 3);
        let request = ImageRequest {
            image: ImageId::default(),
            area: LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 5.0,
                height: 5.0,
            },
            fit: ImageFit::Fill,
            sampling: ImageSampling::Nearest,
            opacity: 1.0,
            colorize: None,
            nine_slice: Some(NineSlice::uniform(1)),
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        };
        let mut buffer = VecBuffer::<Xrgb8888>::new(5, 5);

        draw(
            &mut buffer,
            &request,
            &texture,
            PhysicalRect {
                x: 0,
                y: 0,
                width: 5,
                height: 5,
            },
            1.0,
        );

        assert_eq!(buffer.pixels()[0].raw(), 0xff0000);
        assert_eq!(buffer.pixels()[4].raw(), 0x0000ff);
        assert_eq!(buffer.pixels()[20].raw(), 0x800000);
        assert_eq!(buffer.pixels()[24].raw(), 0x000080);
        assert_eq!(buffer.pixels()[12].raw(), 0xff00ff);
    }
}
