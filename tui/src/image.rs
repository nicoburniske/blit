use std::rc::Rc;

use blit::{Atom, AtomCx, Constraints, Paint, PhysicalRect, Size};

use crate::Context;

blit::builder! {
    /// Kitty image placed in a cell area
    #[derive(Clone, Debug, PartialEq)]
    pub struct Image {
        new(#[into] image: ImageHandle),
        #[into]
        intrinsic: Option<Size<i32>> = None,
        fit: ImageFit = ImageFit::Fill,
    }
}

impl<C> Atom<C> for Image
where
    C: Context + Paint<Self>,
{
    fn measure(&self, cx: &mut AtomCx<'_, C>, constraints: Constraints<i32>) -> Size<i32> {
        let cell = cx.context_mut().as_mut().renderer.cell_size();
        let pixels = self.image.size();
        let width = f64::from(pixels.width) / f64::from(cell.width);
        let height = f64::from(pixels.height) / f64::from(cell.height);
        let mut size = self
            .intrinsic
            .unwrap_or(Size::new((width.round() as i32).max(1), (height.round() as i32).max(1)));
        if self.fit == ImageFit::Contain {
            let scale = (f64::from(size.width.min(constraints.max.width)) / width)
                .min(f64::from(size.height.min(constraints.max.height)) / height);
            if scale <= 0.0 {
                return constraints.constrain(Size::ZERO);
            }
            size = Size::new(
                ((width * scale).round() as i32).max(1),
                ((height * scale).round() as i32).max(1),
            );
        }
        constraints.constrain(size)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: PhysicalRect) {
        cx.context_mut().paint(area, self.clone());
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageFit {
    #[default]
    Fill,
    Contain,
}

#[derive(Clone, Debug)]
pub struct ImageHandle(Rc<ImageInner>);

impl PartialEq for ImageHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl From<&ImageHandle> for ImageHandle {
    fn from(image: &ImageHandle) -> Self {
        image.clone()
    }
}

impl ImageHandle {
    #[doc(hidden)]
    pub fn new(id: ImageId, size: Size<u32>) -> Self {
        Self(Rc::new(ImageInner { id, size }))
    }

    pub fn id(&self) -> ImageId {
        self.0.id
    }

    pub fn size(&self) -> Size<u32> {
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
    pub size: Size<u32>,
    pub format: ImageFormat,
}

impl ImageData {
    pub fn new(pixels: ImagePixels, format: ImageFormat, width: u32, height: u32) -> Self {
        Self {
            pixels,
            size: Size { width, height },
            format,
        }
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

pub type ImageId = u64;

#[derive(Debug)]
struct ImageInner {
    id: ImageId,
    size: Size<u32>,
}
