use blit::PhysicalRect;
use blit_gui::image::{ImageData, ImageRequest, prepare_image_patches};

use super::image_patch::Prepared;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PixelBuffer, VecBuffer, Xrgb8888, render::image_patch::AlphaRows};
    use blit::LogicalRect;
    use blit_gui::{
        color::Color,
        image::{
            ImageFit, ImageFormat, ImageId, ImagePixels, ImageSampling, ImageTiling, NineSlice,
        },
    };

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
                    image.draw_line(
                        buffer.line_mut(y as usize),
                        texture,
                        &alpha_rows,
                        clip,
                        screen.x,
                        y,
                    );
                }
            }
        });
    }

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
        let texture = ImageData::new(
            ImagePixels::Static(&PIXELS),
            ImageFormat::Rgba8Premultiplied,
            1,
            1,
        );
        let request = ImageRequest {
            image: ImageId(0),
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

        assert_eq!(
            buffer.pixels(),
            [0x800000, 0x800000, 0x800000].map(Xrgb8888::from_raw)
        );
        let mut opaque = None;
        prepare(&request, &texture, clip, 1.0, |image, _| {
            opaque = Some(image.is_opaque(&texture, true));
        });
        assert_eq!(opaque, Some(false));
    }

    #[test]
    fn bilinear_image_interpolates_source_pixels() {
        static PIXELS: [u8; 8] = [255, 0, 0, 255, 0, 0, 255, 255];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgba8, 2, 1);
        let request = ImageRequest {
            image: ImageId(0),
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

        assert_eq!(
            buffer.pixels(),
            [0xff0000, 0x800080, 0x0000ff].map(Xrgb8888::from_raw)
        );
    }

    #[test]
    fn unscaled_premultiplied_image_applies_opacity() {
        static PIXELS: [u8; 8] = [255, 0, 0, 255, 0, 128, 0, 128];
        let texture = ImageData::new(
            ImagePixels::Static(&PIXELS),
            ImageFormat::Rgba8Premultiplied,
            2,
            1,
        );
        let request = ImageRequest {
            image: ImageId(0),
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

        assert_eq!(
            buffer.pixels(),
            [0x800000, 0x004000].map(Xrgb8888::from_raw)
        );
    }

    #[test]
    fn image_repeats_horizontally() {
        static PIXELS: [u8; 6] = [255, 0, 0, 0, 0, 255];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgb8, 2, 1);
        let request = ImageRequest {
            image: ImageId(0),
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
            image: ImageId(0),
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
            255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0, 255, 0, 255, 0, 255, 255, 128, 0, 0, 0,
            128, 0, 0, 0, 128,
        ];
        let texture = ImageData::new(ImagePixels::Static(&PIXELS), ImageFormat::Rgb8, 3, 3);
        let request = ImageRequest {
            image: ImageId(0),
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
