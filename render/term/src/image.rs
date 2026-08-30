//! image data and Kitty graphics resources

use std::rc::Rc;

use blit::{LogicalRect, PhysicalRect, PhysicalSize};

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
        let image = Self {
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
        image.validate();
        image
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
pub struct Rgba8 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Rgba8 {
    pub const fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Rgb8,
    Luma8,
    Rgba8,
    Rgba8Premultiplied,
    Alpha8(Rgba8),
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

blit::builder! {
    /// Kitty image placement for a resolved logical area
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct ImagePlacement {
        new(image: ImageId, area: LogicalRect),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u64);

#[derive(Debug)]
struct ImageInner {
    id: ImageId,
    size: PhysicalSize,
}
