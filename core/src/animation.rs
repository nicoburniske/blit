use std::time::Duration;

use crate::interact::WidgetId;

/// transition between layout rectangles resolved for an identified node
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    pub duration: Duration,
    pub easing: Easing,
    pub properties: TransitionProperties,
}

impl Transition {
    pub const fn new(duration: Duration) -> Self {
        Self {
            duration,
            easing: Easing::Linear,
            properties: TransitionProperties::NONE,
        }
    }

    pub const fn easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    pub const fn x(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::X);
        self
    }

    pub const fn y(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::Y);
        self
    }

    pub const fn width(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::WIDTH);
        self
    }

    pub const fn height(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::HEIGHT);
        self
    }

    /// animates placement relative to the current parent
    ///
    /// changing parents may snap rather than preserve the global position
    pub const fn position(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::POSITION);
        self
    }

    pub const fn size(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::SIZE);
        self
    }

    pub const fn layout(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::LAYOUT);
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransitionProperties(u8);

impl TransitionProperties {
    pub const NONE: Self = Self(0);
    pub const X: Self = Self(1 << 0);
    pub const Y: Self = Self(1 << 1);
    pub const WIDTH: Self = Self(1 << 2);
    pub const HEIGHT: Self = Self(1 << 3);
    pub const POSITION: Self = Self(Self::X.0 | Self::Y.0);
    pub const SIZE: Self = Self(Self::WIDTH.0 | Self::HEIGHT.0);
    pub const LAYOUT: Self = Self(Self::POSITION.0 | Self::SIZE.0);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Easing {
    #[default]
    Linear,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
}

impl Easing {
    pub fn apply(self, value: f32) -> f32 {
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
    pub start: f32,
    pub target: f32,
    pub started_at: Option<Duration>,
    pub duration: Duration,
    pub easing: Easing,
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
