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

pub(crate) struct AnimationState {
    pub id: WidgetId,
    pub value: f32,
    start: f32,
    target: f32,
    started_at: Option<Duration>,
    duration: Duration,
    easing: Easing,
    pub previous_bounds: Option<PhysicalRect>,
    pub current_bounds: Option<PhysicalRect>,
    pub active: bool,
    pub damage: bool,
    pub changed: bool,
    pub seen: bool,
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
            current_bounds: None,
            active: false,
            damage: false,
            changed: false,
            seen: false,
        }
    }

    pub fn begin_frame(&mut self) {
        self.current_bounds = None;
        self.damage = false;
        self.changed = false;
        self.seen = false;
    }

    pub fn advance(&mut self, target: f32, duration: Duration, easing: Easing, now: Duration) {
        let was_active = self.active;
        if let Some(started_at) = self.started_at {
            let progress = if self.duration.is_zero() {
                1.0
            } else {
                now.saturating_sub(started_at).as_secs_f32() / self.duration.as_secs_f32()
            }
            .clamp(0.0, 1.0);
            self.value = self.start + (self.target - self.start) * self.easing.apply(progress);
            if progress == 1.0 {
                self.value = self.target;
                self.started_at = None;
                self.active = false;
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
                self.active = false;
            } else {
                self.started_at = Some(now);
                self.active = true;
            }
        }

        self.damage = was_active || self.active || target_changed;
        self.changed = target_changed;
        self.seen = true;
    }
}
