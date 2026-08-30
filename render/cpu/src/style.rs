//! CPU rectangle styling

use crate::color::Color;

blit::builder! {
    /// box shadow relative to a node's resolved bounds
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Shadow {
        new(color: Color),
        offset_x: f32 = 0.0,
        offset_y: f32 = 0.0,
        blur: f32 = 0.0,
        spread: f32 = 0.0,
    }
}

impl Shadow {
    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Border<'a> {
    #[default]
    None,
    Solid {
        width: f32,
        color: Color,
    },
    Gradient {
        width: f32,
        gradient: LinearGradient<'a>,
    },
}

impl<'a> Border<'a> {
    pub const fn solid(width: f32, color: Color) -> Self {
        Self::Solid { width, color }
    }

    pub const fn gradient(width: f32, gradient: LinearGradient<'a>) -> Self {
        Self::Gradient { width, gradient }
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct BorderRadius {
        new(),
        top_left: f32 = 0.0,
        top_right: f32 = 0.0,
        bottom_right: f32 = 0.0,
        bottom_left: f32 = 0.0,
    }
}
impl BorderRadius {
    pub const fn uniform(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearGradient<'a> {
    pub stops: &'a [GradientStop],
    pub angle_degrees: f32,
}

impl<'a> LinearGradient<'a> {
    pub const fn new(stops: &'a [GradientStop]) -> Self {
        Self {
            stops,
            angle_degrees: 0.0,
        }
    }

    pub const fn angle(mut self, angle: f32) -> Self {
        self.angle_degrees = angle;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub position: f32,
    pub color: Color,
}

impl GradientStop {
    pub const fn new(position: f32, color: Color) -> Self {
        Self { position, color }
    }
}
