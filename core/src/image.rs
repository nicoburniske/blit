//! image data, display options, and renderer resources

use std::rc::Rc;

use crate::{
    color::Color,
    geometry::{LogicalRect, LogicalSize, PhysicalRect, PhysicalSize},
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

    #[doc(hidden)]
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

/// frame-local image content and its intrinsic size
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageContent {
    pub image: ImageId,
    pub intrinsic: LogicalSize,
    pub fit: ImageFit,
    pub sampling: ImageSampling,
    pub opacity: f32,
    pub colorize: Option<Color>,
    pub nine_slice: Option<NineSlice>,
    pub horizontal_tiling: ImageTiling,
    pub vertical_tiling: ImageTiling,
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
