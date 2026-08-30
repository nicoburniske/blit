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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
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

impl Shadow {
    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }
}
