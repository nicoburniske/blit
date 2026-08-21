//! box styling and clipping

use crate::{
    color::Color,
    paint::{Border, BorderRadius, LinearGradient},
};

crate::builder! {
    /// paint emitted for a node's resolved bounds
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Style<'a> {
        new(),
        background: Color = Color::TRANSPARENT,
        border: Border<'a> = Border::None,
        radius: BorderRadius = BorderRadius::default(),
        opacity: f32 = 1.0,
        shadow: Option<Shadow> = None,
    }
}

crate::builder! {
    /// box shadow relative to a node's resolved bounds
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Shadow {
        new(color: Color),
        radius: BorderRadius = BorderRadius::default(),
        offset_x: f32 = 0.0,
        offset_y: f32 = 0.0,
        blur: f32 = 0.0,
        spread: f32 = 0.0,
    }
}

/// clipping applied to a node's descendants
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Clip {
    #[default]
    None,
    Bounds,
    Rounded(BorderRadius),
}

impl<'a> Style<'a> {
    pub const fn solid_border(mut self, width: f32, color: Color) -> Self {
        self.border = Border::Solid { width, color };
        self
    }

    pub const fn gradient_border(mut self, width: f32, gradient: LinearGradient<'a>) -> Self {
        self.border = Border::Gradient { width, gradient };
        self
    }
}

impl Default for Style<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Shadow {
    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }
}
