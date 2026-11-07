use std::{
    ops::{BitOr, BitOrAssign},
    time::Duration,
};

use crate::builder;

/// target motion using a tween or a spring
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Animation {
    Tween(Tween),
    Spring(Spring),
}

impl Default for Animation {
    fn default() -> Self {
        Self::Tween(Tween::default())
    }
}

impl From<Tween> for Animation {
    fn from(tween: Tween) -> Self {
        Self::Tween(tween)
    }
}

impl From<Spring> for Animation {
    fn from(spring: Spring) -> Self {
        Self::Spring(spring)
    }
}

builder! {
    /// controls how scalar motion starts and retargets
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct AnimationOptions {
        new(),
        /// discards carried velocity when retargeting
        restart: bool = false,
        /// initial spring speed or fading tween momentum in coordinate units per second
        /// `None` carries the current velocity unless restarting
        /// `Some(v)` overrides it even when `v` is zero or `restart` is true
        /// zero removes carried momentum but a tween still follows its easing
        #[into]
        velocity: Option<f32> = None,
    }
}

builder! {
    /// an easing curve with a duration specifying the time to reach the target
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Tween {
        new(duration: Duration, easing: Easing),
    }
}

builder! {
    /// a linear spring tuned by duration and bounce
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Spring {
        new(
            /// undamped period controlling the response speed
            duration: Duration,
            /// positive overshoots, zero is critically damped and negative is overdamped
            bounce: f32,
        ),
        rest_distance: f32 = 0.001,
        rest_speed: f32 = 0.001,
    }
}

impl Spring {
    /// stop when both distance and speed fall within these absolute tolerances
    pub const fn rest(mut self, distance: f32, speed: f32) -> Self {
        self.rest_distance = distance;
        self.rest_speed = speed;
        self
    }
}

builder! {
    /// applies an animation to selected layout properties
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Motion {
        new(#[into] animation: Animation),
        properties: MotionProps = MotionProps::NONE,
        /// discards carried velocity when retargeting
        restart: bool = false,
    }
}

impl Motion {
    /// animate during window resize instead of snapping to the new layout
    pub fn resize(mut self) -> Self {
        self.properties |= MotionProps::RESIZE;
        self
    }

    pub fn x(mut self) -> Self {
        self.properties |= MotionProps::X;
        self
    }

    pub fn y(mut self) -> Self {
        self.properties |= MotionProps::Y;
        self
    }

    pub fn width(mut self) -> Self {
        self.properties |= MotionProps::WIDTH;
        self
    }

    pub fn height(mut self) -> Self {
        self.properties |= MotionProps::HEIGHT;
        self
    }

    pub fn position(mut self) -> Self {
        self.properties |= MotionProps::POSITION;
        self
    }

    pub fn size(mut self) -> Self {
        self.properties |= MotionProps::SIZE;
        self
    }

    pub fn layout(mut self) -> Self {
        self.properties |= MotionProps::LAYOUT;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MotionProps(u8);

impl MotionProps {
    pub const NONE: Self = Self(0);
    pub const X: Self = Self(1 << 0);
    pub const Y: Self = Self(1 << 1);
    pub const WIDTH: Self = Self(1 << 2);
    pub const HEIGHT: Self = Self(1 << 3);
    pub const RESIZE: Self = Self(1 << 4);
    pub const POSITION: Self = Self(Self::X.0 | Self::Y.0);
    pub const SIZE: Self = Self(Self::WIDTH.0 | Self::HEIGHT.0);
    pub const LAYOUT: Self = Self(Self::POSITION.0 | Self::SIZE.0);

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for MotionProps {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitOrAssign for MotionProps {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

/// easing as a cubic bezier or a function
#[derive(Clone, Copy, Debug)]
pub enum Easing {
    /// control points `[x1, y1, x2, y2]`
    /// x coordinates are between zero and one and y may overshoot
    Bezier([f32; 4]),
    /// receives progress in `0..=1` and returns `(value, slope)`
    /// value must start at zero and end at one
    /// slope is the derivative with respect to progress
    Function(fn(f32) -> (f32, f32)),
}

impl Easing {
    pub const LINEAR: Self = Self::Bezier([0.0, 0.0, 1.0, 1.0]);
    pub const EASE_IN_QUAD: Self = Self::Bezier([1.0 / 3.0, 0.0, 2.0 / 3.0, 1.0 / 3.0]);
    pub const EASE_OUT_QUAD: Self = Self::Bezier([1.0 / 3.0, 2.0 / 3.0, 2.0 / 3.0, 1.0]);
    pub const EASE: Self = Self::Bezier([0.25, 0.1, 0.25, 1.0]);
    pub const EASE_IN: Self = Self::Bezier([0.42, 0.0, 1.0, 1.0]);
    pub const EASE_OUT: Self = Self::Bezier([0.0, 0.0, 0.58, 1.0]);
    pub const EASE_IN_OUT: Self = Self::Bezier([0.42, 0.0, 0.58, 1.0]);

    /// returns the eased value and its derivative
    /// bezier slopes are capped at 1000 in either direction
    #[inline]
    pub fn sample(self, value: f32) -> (f32, f32) {
        const MAX_SLOPE: f64 = 1000.0;

        let [x1, y1, x2, y2] = match self {
            Self::Bezier(points) => points,
            Self::Function(sample) => return sample(value),
        };
        if x1 == y1 && x2 == y2 {
            return (value, 1.0);
        }
        let [y1, y2] = [y1, y2].map(f64::from);
        let value = value as f64;
        let cy = 3.0 * y1;
        let by = 3.0 * (y2 - y1) - cy;
        let ay = 1.0 - cy - by;
        if x1 == 1.0 / 3.0 && x2 == 2.0 / 3.0 {
            // a linear time axis needs no inversion
            let amount = if value <= 0.0 {
                0.0
            } else if value >= 1.0 {
                1.0
            } else {
                ((ay * value + by) * value + cy) * value
            };
            let slope = ((3.0 * ay * value + 2.0 * by) * value + cy).clamp(-MAX_SLOPE, MAX_SLOPE);
            return (amount as f32, slope as f32);
        }
        let [x1, x2] = [x1, x2].map(f64::from);
        let cx = 3.0 * x1;
        let bx = 3.0 * (x2 - x1) - cx;
        let ax = 1.0 - cx - bx;
        let mut low = 0.0;
        let mut high = 1.0;
        let mut t = value.clamp(0.0, 1.0);
        for _ in 0..32 {
            let error = ((ax * t + bx) * t + cx) * t - value;
            let derivative = (3.0 * ax * t + 2.0 * bx) * t + cx;
            if error.abs() <= derivative.abs() * 1e-10 {
                break;
            }
            if error < 0.0 {
                low = t;
            } else {
                high = t;
            }
            let next = t - error / derivative;
            t = if next > low && next < high {
                next
            } else {
                (low + high) * 0.5
            };
        }
        let dx = (3.0 * ax * t + 2.0 * bx) * t + cx;
        let dy = (3.0 * ay * t + 2.0 * by) * t + cy;
        let slope = if dx > 0.0 {
            dy / dx
        } else if t == 0.0 && y1 == 0.0 && x2 > 0.0 {
            y2 / x2
        } else if t == 1.0 && y2 == 1.0 && x1 < 1.0 {
            (1.0 - y1) / (1.0 - x1)
        } else if dy == 0.0 {
            0.0
        } else {
            dy.signum() * MAX_SLOPE
        }
        .clamp(-MAX_SLOPE, MAX_SLOPE);
        let amount = if value <= 0.0 {
            0.0
        } else if value >= 1.0 {
            1.0
        } else {
            ((ay * t + by) * t + cy) * t
        };
        (amount as f32, slope as f32)
    }
}

impl PartialEq for Easing {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Bezier(a), Self::Bezier(b)) => a == b,
            (Self::Function(a), Self::Function(b)) => std::ptr::fn_addr_eq(*a, *b),
            _ => false,
        }
    }
}

impl Default for Easing {
    fn default() -> Self {
        Self::LINEAR
    }
}
