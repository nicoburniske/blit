//! image data and Kitty graphics resources

use std::rc::Rc;

use blit::{LogicalRect, PhysicalSize};

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
    pub format: ImageFormat,
}

impl ImageData {
    pub fn new(pixels: ImagePixels, format: ImageFormat, width: usize, height: usize) -> Self {
        let width = i32::try_from(width).expect("image width is too large");
        let height = i32::try_from(height).expect("image height is too large");
        let image = Self {
            pixels,
            size: PhysicalSize { width, height },
            format,
        };
        image.validate();
        image
    }

    pub fn validate(&self) {
        assert!(self.size.width > 0 && self.size.height > 0);
        let len = (self.size.width as usize)
            .checked_mul(self.size.height as usize)
            .and_then(|pixels| pixels.checked_mul(self.format.bytes_per_pixel()))
            .expect("image data is too large");
        assert_eq!(len, self.pixels.bytes().len());
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
    Rgba8,
}

impl ImageFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgb8 => 3,
            Self::Rgba8 => 4,
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
