//! image data, display options, and renderer resources

use std::rc::Rc;

use crate::color::Color;
use blit::{
    Scale2,
    geometry::{LogicalRect, PhysicalRect, PhysicalSize},
};

#[derive(Clone, Debug)]
pub struct ImageHandle(Rc<ImageInner>);

impl ImageHandle {
    #[doc(hidden)]
    pub fn new(id: ImageId, size: PhysicalSize) -> Self {
        Self(Rc::new(ImageInner { id, size }))
    }

    pub fn id(&self) -> ImageId {
        self.0.id
    }

    pub fn size(&self) -> PhysicalSize {
        self.0.size
    }

    /// returns true when this is the only remaining handle to the image
    pub fn is_uniquely_owned(&self) -> bool {
        Rc::strong_count(&self.0) == 1
    }
}

#[derive(Debug, PartialEq)]
pub struct ImageData {
    pub pixels: ImagePixels,
    pub size: PhysicalSize,
    pub texture_rect: PhysicalRect,
    pub stride_bytes: usize,
    pub format: ImageFormat,
}

impl ImageData {
    pub fn new(pixels: ImagePixels, format: ImageFormat, width: usize, height: usize) -> Self {
        let width = i32::try_from(width).expect("image width is too large");
        let height = i32::try_from(height).expect("image height is too large");
        let stride_bytes = (width as usize)
            .checked_mul(format.bytes_per_pixel())
            .expect("image width is too large");
        let texture = Self {
            pixels,
            size: PhysicalSize { width, height },
            texture_rect: PhysicalRect {
                x: 0,
                y: 0,
                width,
                height,
            },
            stride_bytes,
            format,
        };
        texture.validate();
        texture
    }

    pub fn validate(&self) {
        assert!(self.size.width > 0 && self.size.height > 0);
        assert!(self.texture_rect.x >= 0 && self.texture_rect.y >= 0);
        assert!(self.texture_rect.width > 0 && self.texture_rect.height > 0);
        assert!(
            self.texture_rect
                .x
                .checked_add(self.texture_rect.width)
                .is_some_and(|right| right <= self.size.width)
        );
        assert!(
            self.texture_rect
                .y
                .checked_add(self.texture_rect.height)
                .is_some_and(|bottom| bottom <= self.size.height)
        );
        let row_bytes = (self.texture_rect.width as usize)
            .checked_mul(self.format.bytes_per_pixel())
            .expect("image row is too large");
        assert!(self.stride_bytes >= row_bytes);
        let len = (self.texture_rect.height as usize - 1)
            .checked_mul(self.stride_bytes)
            .and_then(|offset| offset.checked_add(row_bytes))
            .expect("image data is too large");
        assert!(len <= self.pixels.bytes().len());
    }
}

#[derive(Debug, PartialEq)]
pub enum ImagePixels {
    Static(&'static [u8]),
    Owned(Box<[u8]>),
}

impl ImagePixels {
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Static(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Rgb8,
    Luma8,
    Rgba8,
    Rgba8Premultiplied,
    Alpha8(Color),
}

impl ImageFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgb8 => 3,
            Self::Rgba8 | Self::Rgba8Premultiplied => 4,
            Self::Luma8 | Self::Alpha8(_) => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageRequest {
    pub image: ImageId,
    pub area: LogicalRect,
    pub fit: ImageFit,
    pub sampling: ImageSampling,
    pub opacity: f32,
    pub colorize: Option<Color>,
    pub nine_slice: Option<NineSlice>,
    pub horizontal_tiling: ImageTiling,
    pub vertical_tiling: ImageTiling,
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImagePatch {
    pub source: PhysicalRect,
    pub display: PhysicalRect,
    pub bounds: PhysicalRect,
    pub horizontal_tiling: ImageTiling,
    pub vertical_tiling: ImageTiling,
}

#[doc(hidden)]
pub fn prepare_image_patches(
    request: &ImageRequest,
    source_size: PhysicalSize,
    scale_factor: f32,
    mut emit: impl FnMut(ImagePatch),
) {
    let geometry = request.area.to_physical(Scale2::uniform(scale_factor));
    let source = PhysicalRect {
        x: 0,
        y: 0,
        width: source_size.width,
        height: source_size.height,
    };
    if geometry.width <= 0
        || geometry.height <= 0
        || source.width <= 0
        || source.height <= 0
        || request.opacity <= 0.0
    {
        return;
    }

    let mut emit = |patch: ImagePatch| {
        if patch.source.width > 0
            && patch.source.height > 0
            && patch.display.width > 0
            && patch.display.height > 0
            && patch.bounds.width > 0
            && patch.bounds.height > 0
        {
            emit(patch);
        }
    };

    if let Some(slice) = request.nine_slice {
        assert!(slice.left as i32 + slice.right as i32 <= source.width);
        assert!(slice.top as i32 + slice.bottom as i32 <= source.height);
        let fit_borders = |first: i32, second: i32, available: i32| {
            if first + second <= available {
                (first, second)
            } else {
                let first =
                    (first as f32 * available as f32 / (first + second) as f32).round() as i32;
                (first, available - first)
            }
        };
        let (left, right) = fit_borders(
            (slice.left as f32 * scale_factor).round() as i32,
            (slice.right as f32 * scale_factor).round() as i32,
            geometry.width,
        );
        let (top, bottom) = fit_borders(
            (slice.top as f32 * scale_factor).round() as i32,
            (slice.bottom as f32 * scale_factor).round() as i32,
            geometry.height,
        );
        let source_x = [
            0,
            slice.left as i32,
            source.width - slice.right as i32,
            source.width,
        ];
        let source_y = [
            0,
            slice.top as i32,
            source.height - slice.bottom as i32,
            source.height,
        ];
        let destination_x = [
            geometry.x,
            geometry.x + left,
            geometry.x + geometry.width - right,
            geometry.x + geometry.width,
        ];
        let destination_y = [
            geometry.y,
            geometry.y + top,
            geometry.y + geometry.height - bottom,
            geometry.y + geometry.height,
        ];
        for row in 0..3 {
            for column in 0..3 {
                let source = PhysicalRect {
                    x: source_x[column],
                    y: source_y[row],
                    width: source_x[column + 1] - source_x[column],
                    height: source_y[row + 1] - source_y[row],
                };
                let display = PhysicalRect {
                    x: destination_x[column],
                    y: destination_y[row],
                    width: destination_x[column + 1] - destination_x[column],
                    height: destination_y[row + 1] - destination_y[row],
                };
                emit(ImagePatch {
                    source,
                    display,
                    bounds: display,
                    horizontal_tiling: if column == 1 {
                        request.horizontal_tiling
                    } else {
                        ImageTiling::None
                    },
                    vertical_tiling: if row == 1 {
                        request.vertical_tiling
                    } else {
                        ImageTiling::None
                    },
                });
            }
        }
        return;
    }

    let tiled = request.horizontal_tiling != ImageTiling::None
        || request.vertical_tiling != ImageTiling::None;
    let display = if tiled {
        geometry
    } else {
        match request.fit {
            ImageFit::Fill => geometry,
            ImageFit::Contain | ImageFit::Cover => {
                let horizontal = geometry.width as f32 / source.width as f32;
                let vertical = geometry.height as f32 / source.height as f32;
                let scale = if request.fit == ImageFit::Contain {
                    horizontal.min(vertical)
                } else {
                    horizontal.max(vertical)
                };
                let width = (source.width as f32 * scale).round().max(1.0) as i32;
                let height = (source.height as f32 * scale).round().max(1.0) as i32;
                PhysicalRect {
                    x: geometry.x + (geometry.width - width) / 2,
                    y: geometry.y + (geometry.height - height) / 2,
                    width,
                    height,
                }
            }
        }
    };
    let bounds = if request.fit == ImageFit::Cover || tiled {
        display.intersection(geometry).unwrap_or_default()
    } else {
        display
    };
    emit(ImagePatch {
        source,
        display,
        bounds,
        horizontal_tiling: request.horizontal_tiling,
        vertical_tiling: request.vertical_tiling,
    });
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NineSlice {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl NineSlice {
    pub const fn uniform(value: u16) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageFit {
    #[default]
    Fill,
    Contain,
    Cover,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageSampling {
    #[default]
    Nearest,
    Bilinear,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageTiling {
    #[default]
    None,
    Repeat,
    Round,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u64);

#[derive(Debug)]
struct ImageInner {
    id: ImageId,
    size: PhysicalSize,
}
