pub use crate::frame::layout::{Children, LayoutCx};

use crate::{
    Platform,
    frame::Sizing,
    geometry::{Constraints, Size},
};

pub trait Layout<R: Platform>: Copy + 'static {
    type Item: Copy + 'static;

    fn layout(&self, cx: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    #[default]
    Vertical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LayoutResolution {
    #[default]
    Continuous,
    Discrete {
        step: Size,
    },
}

impl LayoutResolution {
    #[inline]
    pub fn extent(self, axis: Axis, value: f32) -> f32 {
        let Self::Discrete { step } = self else {
            return value;
        };
        if value <= 0.0 || !value.is_finite() {
            return value;
        }
        let step = match axis {
            Axis::Horizontal => step.width,
            Axis::Vertical => step.height,
        };
        assert!(step.is_finite() && step > 0.0);
        (value / step).ceil() * step
    }

    #[inline]
    pub fn sizing(self, axis: Axis, sizing: Sizing) -> Sizing {
        match sizing {
            Sizing::Fit { min, max } => Sizing::Fit {
                min: self.extent(axis, min),
                max: self.extent(axis, max),
            },
            Sizing::Grow { min, max } => Sizing::Grow {
                min: self.extent(axis, min),
                max: self.extent(axis, max),
            },
            Sizing::Fixed(size) => Sizing::Fixed(self.extent(axis, size)),
            Sizing::Percent(fraction) => Sizing::Percent(fraction),
        }
    }
}
