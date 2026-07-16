use crate::{color::Color, geometry::LogicalRect, resource::ImageId};

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NineSlice {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl NineSlice {
    pub const fn uniform(value: u16) -> Self { Self { top: value, right: value, bottom: value, left: value } }
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
