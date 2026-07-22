//! image and string resources managed by the rendering platform

use std::{
    cell::OnceCell,
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
    rc::Rc,
};

use crate::{
    color::Color,
    geometry::{PhysicalRect, PhysicalSize},
    platform::{Platform, PlatformImpl},
};

// image

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
        let stride_bytes =
            (width as usize).checked_mul(format.bytes_per_pixel()).expect("image width is too large");
        let texture = Self {
            pixels,
            size: PhysicalSize { width, height },
            texture_rect: PhysicalRect { x: 0, y: 0, width, height },
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
        assert!(self
            .texture_rect
            .x
            .checked_add(self.texture_rect.width)
            .is_some_and(|right| right <= self.size.width));
        assert!(self
            .texture_rect
            .y
            .checked_add(self.texture_rect.height)
            .is_some_and(|bottom| bottom <= self.size.height));
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
    platform: Option<NonNull<dyn PlatformImpl>>,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl ImageHandle {
    pub fn empty() -> Self {
        Self {
            id: ImageId(0),
            size: PhysicalSize { width: 0, height: 0 },
            platform: None,
            not_send_or_sync: PhantomData,
        }
    }

    pub(crate) fn new(id: ImageId, size: PhysicalSize, platform: NonNull<dyn PlatformImpl>) -> Self {
        Self { id, size, platform: Some(platform), not_send_or_sync: PhantomData }
    }

    pub fn id(&self) -> ImageId { self.id }

    pub fn size(&self) -> PhysicalSize { self.size }

    pub fn is_empty(&self) -> bool { self.platform.is_none() }
}

impl Default for ImageHandle {
    fn default() -> Self { Self::empty() }
}

impl Drop for ImageHandle {
    fn drop(&mut self) {
        if let Some(mut platform) = self.platform {
            unsafe { platform.as_mut().drop_image(self.id) }
        }
    }
}

// string

#[derive(Debug, PartialEq, Eq)]
pub enum StringData {
    Owned(Box<str>),
    Static(&'static str),
}

impl AsRef<str> for StringData {
    fn as_ref(&self) -> &str {
        match self {
            Self::Owned(string) => string,
            Self::Static(string) => string,
        }
    }
}

impl From<Box<str>> for StringData {
    fn from(string: Box<str>) -> Self { Self::Owned(string) }
}

impl From<String> for StringData {
    fn from(string: String) -> Self { Self::Owned(string.into_boxed_str()) }
}

impl From<&'static str> for StringData {
    fn from(string: &'static str) -> Self { Self::Static(string) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StringId(pub u64);

#[derive(Clone, Copy, Debug)]
pub enum TextSource {
    Resource(StringId),
    Static(&'static str),
}

impl PartialEq for TextSource {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::Resource(left), Self::Resource(right)) => left == right,
            (Self::Static(left), Self::Static(right)) => {
                left.as_ptr() == right.as_ptr() && left.len() == right.len()
            }
            _ => false,
        }
    }
}

impl Eq for TextSource {}

impl Hash for TextSource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match *self {
            Self::Resource(string) => {
                0_u8.hash(state);
                string.hash(state);
            }
            Self::Static(string) => {
                1_u8.hash(state);
                string.as_ptr().hash(state);
                string.len().hash(state);
            }
        }
    }
}

impl From<StringId> for TextSource {
    fn from(string: StringId) -> Self { Self::Resource(string) }
}

impl From<&StringHandle> for TextSource {
    fn from(string: &StringHandle) -> Self { Self::Resource(string.id()) }
}

impl From<&mut StringHandle> for TextSource {
    fn from(string: &mut StringHandle) -> Self { Self::Resource(string.id()) }
}

impl From<&'static str> for TextSource {
    fn from(string: &'static str) -> Self { Self::Static(string) }
}

#[derive(Debug)]
pub struct StringHandle {
    id: StringId,
    platform: Platform,
}

impl StringHandle {
    pub(crate) fn new(id: StringId, platform: Platform) -> Self { Self { id, platform } }

    pub fn id(&self) -> StringId { self.id }

    pub fn replace(&mut self, string: impl Into<StringData>) {
        let platform = self.platform.inner();
        let old = self.id;
        self.id = platform.create_string(string.into());
        platform.drop_string(old);
    }

    pub fn edit(&mut self) -> StringGuard<'_> {
        StringGuard { handle: self, string: OnceCell::new(), dirty: false }
    }
}

impl AsRef<StringHandle> for StringHandle {
    fn as_ref(&self) -> &StringHandle { self }
}

impl Deref for StringHandle {
    type Target = str;

    fn deref(&self) -> &Self::Target { self.platform.inner().string(self.id) }
}

impl fmt::Display for StringHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result { formatter.write_str(self) }
}

impl Drop for StringHandle {
    fn drop(&mut self) {
        // safety: the platform outlives handles created by its Runtime
        self.platform.inner().drop_string(self.id)
    }
}

pub struct StringGuard<'a> {
    handle: &'a mut StringHandle,
    string: OnceCell<String>,
    dirty: bool,
}

impl Deref for StringGuard<'_> {
    type Target = String;

    fn deref(&self) -> &Self::Target { self.string.get_or_init(|| (**self.handle).to_owned()) }
}

impl DerefMut for StringGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        if self.string.get().is_none() {
            self.string.set((**self.handle).to_owned()).unwrap();
        }
        self.dirty = true;
        self.string.get_mut().unwrap()
    }
}

impl Drop for StringGuard<'_> {
    fn drop(&mut self) {
        if self.dirty {
            self.handle.replace(self.string.take().unwrap());
        }
    }
}
