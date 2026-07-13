use std::{marker::PhantomData, ptr::NonNull, rc::Rc};

use crate::{Color, PhysicalRect, PhysicalSize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Rgb8,
    Rgba8,
    Rgba8Premultiplied,
    Alpha8(Color),
}

impl ImageFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgb8 => 3,
            Self::Rgba8 | Self::Rgba8Premultiplied => 4,
            Self::Alpha8(_) => 1,
        }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u64);

pub struct ImageHandle {
    id: ImageId,
    size: PhysicalSize,
    data: NonNull<()>,
    drop_image: Option<unsafe fn(NonNull<()>, ImageId)>,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl ImageHandle {
    pub fn empty() -> Self {
        Self {
            id: ImageId(0),
            size: PhysicalSize {
                width: 0,
                height: 0,
            },
            data: NonNull::dangling(),
            drop_image: None,
            not_send_or_sync: PhantomData,
        }
    }

    pub(crate) fn new(
        id: ImageId,
        size: PhysicalSize,
        data: NonNull<()>,
        drop_image: unsafe fn(NonNull<()>, ImageId),
    ) -> Self {
        Self {
            id,
            size,
            data,
            drop_image: Some(drop_image),
            not_send_or_sync: PhantomData,
        }
    }

    pub fn id(&self) -> ImageId {
        self.id
    }

    pub fn size(&self) -> PhysicalSize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.drop_image.is_none()
    }
}

impl Default for ImageHandle {
    fn default() -> Self {
        Self::empty()
    }
}

impl Drop for ImageHandle {
    fn drop(&mut self) {
        if let Some(drop_image) = self.drop_image {
            unsafe { drop_image(self.data, self.id) }
        }
    }
}
