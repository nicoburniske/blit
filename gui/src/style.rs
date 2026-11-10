//! drawing fills and rectangle styling

use crate::color::Color;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill<'a> {
    Solid(Color),
    LinearGradient(LinearGradient<'a>),
    ConicGradient(ConicGradient<'a>),
}

impl Fill<'_> {
    /// resolves malformed gradients to a solid fill and nonfinite geometry to defaults
    pub fn normalized(self) -> Self {
        let (stops, fill) = match self {
            Self::Solid(_) => return self,
            Self::LinearGradient(mut gradient) => {
                gradient.angle_degrees = if gradient.angle_degrees.is_finite() {
                    gradient.angle_degrees.rem_euclid(360.0)
                } else {
                    0.0
                };
                (gradient.stops, Self::LinearGradient(gradient))
            }
            Self::ConicGradient(mut gradient) => {
                gradient.angle_degrees = if gradient.angle_degrees.is_finite() {
                    gradient.angle_degrees.rem_euclid(360.0)
                } else {
                    0.0
                };
                gradient.center = gradient.center.map(|value| if value.is_finite() { value } else { 0.5 });
                (gradient.stops, Self::ConicGradient(gradient))
            }
        };
        if stops.len() >= 2
            && stops
                .iter()
                .all(|stop| stop.position.is_finite() && (0.0..=1.0).contains(&stop.position))
            && stops.windows(2).all(|stops| stops[0].position < stops[1].position)
        {
            fill
        } else {
            Self::Solid(stops.first().map_or(Color::TRANSPARENT, |stop| stop.color))
        }
    }
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

impl<'a> From<ConicGradient<'a>> for Fill<'a> {
    fn from(gradient: ConicGradient<'a>) -> Self {
        Self::ConicGradient(gradient)
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

    pub fn gradient(width: f32, gradient: impl Into<Fill<'a>>) -> Self {
        Self {
            width,
            fill: gradient.into(),
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

/// a clockwise sweep with zero degrees pointing right and a center relative to the rectangle
/// the exact center uses the first stop
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConicGradient<'a> {
    pub stops: &'a [GradientStop],
    pub center: [f32; 2],
    pub angle_degrees: f32,
}

impl<'a> ConicGradient<'a> {
    pub const fn new(stops: &'a [GradientStop]) -> Self {
        Self {
            stops,
            center: [0.5, 0.5],
            angle_degrees: 0.0,
        }
    }

    pub const fn center(mut self, x: f32, y: f32) -> Self {
        self.center = [x, y];
        self
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
