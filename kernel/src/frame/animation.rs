use std::time::Duration;

use crate::{animation::Easing, interact::WidgetId};

pub struct AnimationState {
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
    pub fn update(
        animations: &mut Vec<Self>,
        id: WidgetId,
        initial: f32,
        advance: impl FnOnce(&mut Self),
    ) -> f32 {
        let index = match animations.binary_search_by_key(&id, |animation| animation.id) {
            Ok(index) => index,
            Err(index) => {
                animations.insert(index, Self::new(id, initial));
                index
            }
        };
        assert!(
            !animations[index].seen,
            "duplicate animation WidgetId {id:?}"
        );
        advance(&mut animations[index]);
        animations[index].value
    }

    fn new(id: WidgetId, target: f32) -> Self {
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

        if self.target != target {
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
        if !self.looping || self.duration != duration || self.easing != easing {
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
