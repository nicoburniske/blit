use crate::{color::Color, geometry::LogicalRect, Ui};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rectangle<'a> {
    pub area: LogicalRect,
    pub background: Color,
    pub border: Border<'a>,
    pub radius: BorderRadius,
    pub opacity: f32,
}

impl<'a> Rectangle<'a> {
    pub fn new(area: LogicalRect) -> Self {
        Self {
            area,
            background: Color::TRANSPARENT,
            border: Border::default(),
            radius: BorderRadius::default(),
            opacity: 1.0,
        }
    }

    pub fn background(mut self, background: Color) -> Self {
        self.background = background;
        self
    }

    pub fn radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    pub fn uniform_radius(mut self, radius: f32) -> Self {
        self.radius =
            BorderRadius { top_left: radius, top_right: radius, bottom_right: radius, bottom_left: radius };
        self
    }

    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border = Border::Solid { width, color };
        self
    }

    pub fn gradient_border(mut self, width: f32, gradient: LinearGradient<'a>) -> Self {
        self.border = Border::Gradient { width, gradient };
        self
    }

    pub fn render(self, ui: &mut Ui) { ui.paint_rectangle(self); }
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearGradient<'a> {
    pub stops: &'a [GradientStop],
    pub angle_degrees: f32,
}

impl<'a> LinearGradient<'a> {
    pub const fn new(stops: &'a [GradientStop]) -> Self { Self { stops, angle_degrees: 0.0 } }

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
    pub const fn new(position: f32, color: Color) -> Self { Self { position, color } }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}
