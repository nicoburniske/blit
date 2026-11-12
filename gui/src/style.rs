//! drawing fills and rectangle styling

use std::hash::{Hash, Hasher};

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
                gradient.angle = if gradient.angle.is_finite() {
                    gradient.angle.rem_euclid(360.0)
                } else {
                    0.0
                };
                (gradient.stops, Self::LinearGradient(gradient))
            }
            Self::ConicGradient(mut gradient) => {
                gradient.angle = if gradient.angle.is_finite() {
                    gradient.angle.rem_euclid(360.0)
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

impl Hash for Fill<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        let (stops, angle) = match *self {
            Fill::Solid(color) => return color.hash(state),
            Fill::LinearGradient(gradient) => (gradient.stops, gradient.angle),
            Fill::ConicGradient(gradient) => {
                gradient
                    .center
                    .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
                    .hash(state);
                (gradient.stops, gradient.angle)
            }
        };
        state.write_u32(if angle == 0.0 { 0 } else { angle.to_bits() });
        stops.len().hash(state);
        for stop in stops {
            let position = stop.position;
            state.write_u32(if position == 0.0 { 0 } else { position.to_bits() });
            stop.color.hash(state);
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

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct LinearGradient<'a> {
        new(stops: &'a [GradientStop]),
        angle: f32 = 0.0,
    }
}

blit::builder! {
    /// a clockwise sweep with zero degrees pointing right and a center relative to the rectangle
    /// the exact center uses the first stop
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct ConicGradient<'a> {
        new(stops: &'a [GradientStop]),
        center: [f32; 2] = [0.5, 0.5],
        angle: f32 = 0.0,
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
