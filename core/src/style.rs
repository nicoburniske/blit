//! box appearance and clipping

use crate::{
    color::Color,
    paint::{Border, BorderRadius, LinearGradient},
};

/// paint emitted for a node's resolved bounds
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style<'a> {
    pub background: Color,
    pub border: Border<'a>,
    pub radius: BorderRadius,
    pub opacity: f32,
    pub shadow: Option<Shadow>,
}

/// box shadow relative to a node's resolved bounds
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub color: Color,
    pub radius: BorderRadius,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
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
    pub const fn new() -> Self {
        Self {
            background: Color::TRANSPARENT,
            border: Border::None,
            radius: BorderRadius {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
            opacity: 1.0,
            shadow: None,
        }
    }

    pub const fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    pub const fn border(mut self, width: f32, color: Color) -> Self {
        self.border = Border::Solid { width, color };
        self
    }

    pub const fn gradient_border(mut self, width: f32, gradient: LinearGradient<'a>) -> Self {
        self.border = Border::Gradient { width, gradient };
        self
    }

    pub const fn radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }

    pub const fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    pub const fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }
}

impl Default for Style<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Shadow {
    pub const fn new(color: Color) -> Self {
        Self {
            color,
            radius: BorderRadius {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
        }
    }

    pub const fn radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }

    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }

    pub const fn blur(mut self, blur: f32) -> Self {
        self.blur = blur;
        self
    }

    pub const fn spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }
}
