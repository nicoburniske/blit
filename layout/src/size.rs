/// one dimensional sizing policy interpreted by a layout
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    Fit { min: f32, max: f32 },
    Grow { min: f32, max: f32 },
    Fixed(f32),
    Percent(f32),
}

impl Sizing {
    pub const fn fit() -> Self {
        Self::fit_range(0.0, f32::INFINITY)
    }

    pub const fn fit_range(min: f32, max: f32) -> Self {
        Self::Fit { min, max }
    }

    pub const fn grow() -> Self {
        Self::grow_range(0.0, f32::INFINITY)
    }

    pub const fn grow_range(min: f32, max: f32) -> Self {
        Self::Grow { min, max }
    }

    pub const fn fixed(size: f32) -> Self {
        Self::Fixed(size)
    }

    pub const fn percent(fraction: f32) -> Self {
        Self::Percent(fraction)
    }

    #[inline]
    pub fn clamp(self, size: f32) -> f32 {
        match self {
            Self::Fit { min, max } | Self::Grow { min, max } => size.clamp(min.max(0.0), max.max(min).max(0.0)),
            Self::Fixed(fixed) => fixed.max(0.0),
            Self::Percent(_) => size.max(0.0),
        }
    }
}
