use std::time::Duration;

use crate::{PhysicalRect, WidgetId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Easing {
    #[default]
    Linear,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
}

impl Easing {
    fn apply(self, value: f32) -> f32 {
        match self {
            Self::Linear => value,
            Self::EaseInQuad => value * value,
            Self::EaseOutQuad => 1.0 - (1.0 - value) * (1.0 - value),
            Self::EaseInOutQuad if value < 0.5 => 2.0 * value * value,
            Self::EaseInOutQuad => 1.0 - (-2.0 * value + 2.0).powi(2) / 2.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub target: f32,
    pub duration: Duration,
    pub easing: Easing,
}

impl Transition {
    pub const fn new(target: f32, duration: Duration, easing: Easing) -> Self {
        Self {
            target,
            duration,
            easing,
        }
    }
}

pub struct AnimationState {
    pub id: WidgetId,
    pub value: f32,
    pub start: f32,
    pub target: f32,
    pub started_at: Option<Duration>,
    pub duration: Duration,
    pub easing: Easing,
    pub previous_bounds: Option<PhysicalRect>,
    pub seen: bool,
    pub looping: bool,
}

impl AnimationState {
    pub fn new(id: WidgetId, target: f32) -> Self {
        Self {
            id,
            value: target,
            start: target,
            target,
            started_at: None,
            duration: Duration::ZERO,
            easing: Easing::Linear,
            previous_bounds: None,
            seen: false,
            looping: false,
        }
    }

    pub fn is_active(&self) -> bool {
        self.started_at.is_some()
    }

    pub fn advance(&mut self, target: f32, duration: Duration, easing: Easing, now: Duration) {
        if self.looping {
            self.start = self.value;
            self.target = self.value;
            self.started_at = None;
            self.looping = false;
        }
        if let Some(started_at) = self.started_at {
            let progress = (now.saturating_sub(started_at).as_secs_f32()
                / self.duration.as_secs_f32())
            .min(1.0);
            self.value = self.start + (self.target - self.start) * self.easing.apply(progress);
            if progress == 1.0 {
                self.value = self.target;
                self.started_at = None;
            }
        }

        let target_changed = self.target != target;
        if target_changed {
            self.start = self.value;
            self.target = target;
            self.duration = duration;
            self.easing = easing;
            if self.start == self.target || duration.is_zero() {
                self.value = target;
                self.started_at = None;
            } else {
                self.started_at = Some(now);
            }
        }

        self.seen = true;
    }

    pub fn advance_loop(&mut self, duration: Duration, easing: Easing, now: Duration) {
        if duration.is_zero() {
            self.advance(0.0, duration, easing, now);
            return;
        }
        let changed = !self.looping || self.duration != duration || self.easing != easing;
        if changed {
            self.started_at = Some(now);
            self.duration = duration;
            self.easing = easing;
            self.looping = true;
        }
        let started_at = self.started_at.unwrap();
        let progress = now.saturating_sub(started_at).as_secs_f32() / duration.as_secs_f32();
        self.value = easing.apply(progress % 1.0);
        self.seen = true;
    }
}
