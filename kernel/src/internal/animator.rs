use std::time::Duration;

use crate::{
    Scalar,
    animation::{Animation, AnimationOptions, Spring, Tween},
};

/// continuous motion with snapped output
#[derive(Clone, Debug)]
pub struct Animator<T, const N: usize> {
    pub values: [T; N],
    pub targets: [T; N],
    pub active: [bool; N],
    pub options: AnimationOptions,
    pub kind: Kind<N>,
}

impl<T: Scalar, const N: usize> Animator<T, N> {
    pub fn new(values: [T; N], animation: impl Into<Animation>, options: AnimationOptions) -> Self {
        Self {
            values,
            targets: values,
            active: [false; N],
            options,
            kind: Kind::new(animation.into(), [0.0; N], [0.0; N], 0.0),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.iter().any(|active| *active)
    }

    pub fn advance(
        &mut self,
        targets: [T; N],
        enabled: [bool; N],
        animation: impl Into<Animation>,
        mut options: AnimationOptions,
        now: Duration,
        commit: bool,
    ) -> ([T; N], [bool; N]) {
        let mut animation = animation.into();
        // normalize motion params
        if let Some(velocity) = &mut options.velocity
            && !velocity.is_finite()
        {
            *velocity = 0.0;
        }
        if let Animation::Spring(spring) = &mut animation {
            for value in [&mut spring.bounce, &mut spring.rest_distance, &mut spring.rest_speed] {
                if !value.is_finite() {
                    *value = 0.0;
                }
            }
            spring.bounce = spring.bounce.clamp(-0.999, 1.0);
            spring.rest_distance = spring.rest_distance.max(f32::EPSILON);
            spring.rest_speed = spring.rest_speed.max(f32::EPSILON);
        }
        let now = now.as_secs_f64();
        let previous_animation = match &self.kind {
            Kind::Tween { tween, .. } => Animation::Tween(*tween),
            Kind::Spring { spring, .. } => Animation::Spring(*spring),
        };
        let (mut displacement, mut velocity, mut active) = ([0.0; N], [0.0; N], [false; N]);
        if self.is_active() {
            (displacement, velocity, active) = self.kind.sample(now);
            for i in 0..N {
                if !self.active[i] {
                    displacement[i] = 0.0;
                    velocity[i] = 0.0;
                    active[i] = false;
                }
            }
        }
        let retarget = previous_animation != animation
            || self.options != options
            || (0..N).any(|i| enabled[i] && targets[i] != self.targets[i]);
        let instant = match animation {
            Animation::Tween(tween) => tween.duration.is_zero(),
            Animation::Spring(spring) => spring.duration.is_zero(),
        };
        let start = std::array::from_fn(|i| targets[i].delta(self.targets[i]) + displacement[i]);
        let values = std::array::from_fn(|i| {
            if !enabled[i] || instant {
                targets[i]
            } else {
                self.targets[i].offset(displacement[i])
            }
        });
        if retarget {
            for i in 0..N {
                velocity[i] = options
                    .velocity
                    .unwrap_or(if options.restart { 0.0 } else { velocity[i] });
                active[i] = start[i] != 0.0 || velocity[i] != 0.0;
            }
        }
        for i in 0..N {
            active[i] &= enabled[i] && !instant;
        }
        if commit {
            if retarget {
                self.kind = Kind::new(animation, start, velocity, now);
            }
            self.options = options;
            self.values = values;
            self.targets = targets;
            self.active = active;
            let (Kind::Tween {
                displacement, velocity, ..
            }
            | Kind::Spring {
                displacement, velocity, ..
            }) = &mut self.kind;
            for (i, enabled) in enabled.into_iter().enumerate() {
                if !enabled || instant {
                    displacement[i] = 0.0;
                    velocity[i] = 0.0;
                }
            }
        }
        (values, active)
    }

    pub fn clamp(&mut self, channel: usize, min: T, max: T, now: Duration) -> T {
        let value = self.values[channel].clamp(min, max);
        if value != self.values[channel] {
            self.values[channel] = value;
            let target = self.targets[channel];
            if (value == min && target < min) || (value == max && target > max) {
                self.targets[channel] = value;
                self.active[channel] = false;
            }
            if let Kind::Spring { spring, .. } = self.kind {
                let (mut displacement, mut velocity, _) = self.kind.sample(now.as_secs_f64());
                displacement[channel] = self.targets[channel].delta(value);
                velocity[channel] = 0.0;
                self.kind = Kind::Spring {
                    spring,
                    displacement,
                    velocity,
                    time: now.as_secs_f64(),
                };
            }
        }
        value
    }
}

#[derive(Clone, Debug)]
pub enum Kind<const N: usize> {
    Tween {
        tween: Tween,
        displacement: [f32; N],
        velocity: [f32; N],
        time: f64,
    },
    Spring {
        spring: Spring,
        displacement: [f32; N],
        velocity: [f32; N],
        time: f64,
    },
}

impl<const N: usize> Kind<N> {
    fn new(animation: Animation, displacement: [f32; N], velocity: [f32; N], time: f64) -> Self {
        match animation {
            Animation::Tween(tween) => Self::Tween {
                tween,
                displacement,
                velocity,
                time,
            },
            Animation::Spring(spring) => Self::Spring {
                spring,
                displacement,
                velocity,
                time,
            },
        }
    }

    #[inline]
    fn sample(&self, now: f64) -> ([f32; N], [f32; N], [bool; N]) {
        match self {
            Self::Tween {
                tween,
                displacement: initial,
                velocity: launch,
                time,
            } => {
                let elapsed = (now - *time).max(0.0);
                let seconds = tween.duration.as_secs_f64();
                if elapsed >= seconds {
                    return ([0.0; N], [0.0; N], [false; N]);
                }
                let progress = (elapsed / seconds) as f32;
                let remaining = 1.0 - progress;
                let (amount, speed) = tween.easing.sample(progress);
                let a = 1.0 - amount;
                let b = elapsed as f32 * remaining * remaining;
                let c = -speed / seconds as f32;
                let d = remaining * (1.0 - 3.0 * progress);
                let (mut displacement, mut velocity, mut active) = ([0.0; N], [0.0; N], [false; N]);
                for i in 0..N {
                    displacement[i] = a * initial[i] + b * launch[i];
                    velocity[i] = c * initial[i] + d * launch[i];
                    active[i] = initial[i] != 0.0 || launch[i] != 0.0;
                }
                (displacement, velocity, active)
            }
            Self::Spring {
                spring,
                displacement: initial,
                velocity: launch,
                time,
            } => {
                let elapsed = (now - *time).max(0.0);
                let omega = std::f64::consts::TAU / spring.duration.as_secs_f64();
                let bounce = spring.bounce as f64;
                let damping = if bounce >= 0.0 {
                    1.0 - bounce
                } else {
                    1.0 / (1.0 + bounce)
                };
                let decay = damping * omega;
                let discriminant = damping * damping - 1.0;
                let (cosine, sine) = if discriminant == 0.0 {
                    let envelope = (-omega * elapsed).exp();
                    (envelope, elapsed * envelope)
                } else if discriminant < 0.0 {
                    let frequency = omega * (-discriminant).sqrt();
                    let (sine, cosine) = (frequency * elapsed).sin_cos();
                    let envelope = (-decay * elapsed).exp();
                    (envelope * cosine, envelope * sine / frequency)
                } else {
                    let root = discriminant.sqrt();
                    let frequency = omega * root;
                    let slow = (-omega / (damping + root) * elapsed).exp();
                    let fast = (-(decay + frequency) * elapsed).exp();
                    let sine = slow * -(-2.0 * frequency * elapsed).exp_m1() / (2.0 * frequency);
                    ((slow + fast) * 0.5, sine)
                };
                let [a, b, c, d] = [
                    (cosine + decay * sine) as f32,
                    sine as f32,
                    (-omega * omega * sine) as f32,
                    (cosine - decay * sine) as f32,
                ];
                let (mut displacement, mut velocity, mut active) = ([0.0; N], [0.0; N], [false; N]);
                for i in 0..N {
                    let position = a * initial[i] + b * launch[i];
                    let speed = c * initial[i] + d * launch[i];
                    active[i] = (initial[i] != 0.0 || launch[i] != 0.0)
                        && (elapsed == 0.0 || position.abs() > spring.rest_distance || speed.abs() > spring.rest_speed);
                    if active[i] {
                        displacement[i] = position;
                        velocity[i] = speed;
                    }
                }
                (displacement, velocity, active)
            }
        }
    }
}
