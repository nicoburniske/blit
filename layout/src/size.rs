use blit::Scalar;

/// one dimensional sizing policy interpreted by a layout
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing<T> {
    Fit { min: T, max: T },
    Grow { min: T, max: T },
    Fixed(T),
    Percent(f32),
}

impl<T: Scalar> Sizing<T> {
    pub const fn fit() -> Self {
        Self::fit_range(T::ZERO, T::UNBOUNDED)
    }
    pub const fn fit_range(min: T, max: T) -> Self {
        Self::Fit { min, max }
    }
    pub const fn grow() -> Self {
        Self::grow_range(T::ZERO, T::UNBOUNDED)
    }
    pub const fn grow_range(min: T, max: T) -> Self {
        Self::Grow { min, max }
    }
    pub const fn fixed(size: T) -> Self {
        Self::Fixed(size)
    }
    pub const fn percent(fraction: f32) -> Self {
        Self::Percent(fraction)
    }

    #[inline]
    pub fn clamp(self, size: T) -> T {
        match self {
            Self::Fit { min, max } | Self::Grow { min, max } => {
                let min = min.max(T::ZERO);
                size.clamp(min, max.max(min))
            }
            Self::Fixed(fixed) => fixed.max(T::ZERO),
            Self::Percent(_) => size.max(T::ZERO),
        }
    }
}
