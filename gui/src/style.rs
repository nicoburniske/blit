//! drawing fills and rectangle styling

use crate::color::Color;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill<'a> {
    Solid(Color),
    LinearGradient(LinearGradient<'a>),
}

impl Default for Fill<'_> {
    fn default() -> Self {
        Self::Solid(Color::TRANSPARENT)
    }
}

impl From<Color> for Fill<'_> {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

impl<'a> From<LinearGradient<'a>> for Fill<'a> {
    fn from(gradient: LinearGradient<'a>) -> Self {
        Self::LinearGradient(gradient)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Border<'a> {
    pub width: f32,
    pub fill: Fill<'a>,
}

impl<'a> Border<'a> {
    pub const fn solid(width: f32, color: Color) -> Self {
        Self {
            width,
            fill: Fill::Solid(color),
        }
    }

    pub const fn gradient(width: f32, gradient: LinearGradient<'a>) -> Self {
        Self {
            width,
            fill: Fill::LinearGradient(gradient),
        }
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
    pub fn uniform(radius: f32) -> Self {
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
