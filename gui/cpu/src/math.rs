//! float math from std when linked, otherwise libm

#[cfg(not(feature = "std"))]
pub use libm::{ceil, ceilf, floor, floorf, round, roundf, sincosf, sqrtf};

#[cfg(feature = "std")]
pub use self::std_math::*;

#[cfg(feature = "std")]
mod std_math {
    pub fn roundf(value: f32) -> f32 {
        value.round()
    }

    pub fn round(value: f64) -> f64 {
        value.round()
    }

    pub fn floorf(value: f32) -> f32 {
        value.floor()
    }

    pub fn floor(value: f64) -> f64 {
        value.floor()
    }

    pub fn ceilf(value: f32) -> f32 {
        value.ceil()
    }

    pub fn ceil(value: f64) -> f64 {
        value.ceil()
    }

    pub fn sqrtf(value: f32) -> f32 {
        value.sqrt()
    }

    pub fn sincosf(value: f32) -> (f32, f32) {
        value.sin_cos()
    }
}
