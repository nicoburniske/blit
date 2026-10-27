use super::*;

pub struct AnimationState {
    pub id: Id,
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
    pub fn update(animations: &mut Vec<Self>, id: Id, initial: f32, advance: impl FnOnce(&mut Self)) -> f32 {
        let index = match animations.binary_search_by_key(&id, |animation| animation.id) {
            Ok(index) => index,
            Err(index) => {
                animations.insert(index, Self::new(id, initial));
                index
            }
        };
        assert!(!animations[index].seen, "duplicate animation Id {id:?}");
        advance(&mut animations[index]);
        animations[index].value
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
            let progress = (now.saturating_sub(started_at).as_secs_f32() / self.duration.as_secs_f32()).min(1.0);
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

impl AnimationState {
    fn new(id: Id, target: f32) -> Self {
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
}

#[derive(Clone, Copy)]
pub struct TransitionState<T> {
    pub id: Id,
    pub current: Rect<T>,
    pub initial: Rect<T>,
    pub target: Rect<T>,
    pub started_at: Option<Duration>,
    pub active: TransitionProperties,
    pub node: NodeId,
    pub config: Transition,
    pub initialized: bool,
    pub seen: bool,
}

impl<T: Scalar> TransitionState<T> {
    pub fn new(id: Id, node: NodeId, config: Transition) -> Self {
        Self {
            id,
            current: Rect::default(),
            initial: Rect::default(),
            target: Rect::default(),
            started_at: None,
            active: TransitionProperties::NONE,
            node,
            config,
            initialized: false,
            seen: true,
        }
    }

    pub fn is_active(&self) -> bool {
        self.started_at.is_some()
    }

    pub fn advance(&mut self, target: Rect<T>, now: Duration, resized: bool) {
        if !self.initialized || (resized && !self.config.properties.intersects(TransitionProperties::RESIZE)) {
            self.current = target;
            self.initial = target;
            self.target = target;
            self.initialized = true;
            self.started_at = None;
            self.active = TransitionProperties::NONE;
            return;
        }
        self.active = self.active.intersection(self.config.properties);
        if self.active.is_empty() {
            self.started_at = None;
        }
        if self.started_at.is_some() && self.config.duration.is_zero() {
            self.current = self.target;
            self.started_at = None;
            self.active = TransitionProperties::NONE;
        }
        if let Some(started_at) = self.started_at {
            let progress = (now.saturating_sub(started_at).as_secs_f32() / self.config.duration.as_secs_f32()).min(1.0);
            let amount = self.config.easing.apply(progress);
            if self.active.intersects(TransitionProperties::X) {
                self.current.x = self.initial.x.lerp(self.target.x, amount);
            }
            if self.active.intersects(TransitionProperties::Y) {
                self.current.y = self.initial.y.lerp(self.target.y, amount);
            }
            if self.active.intersects(TransitionProperties::WIDTH) {
                self.current.width = self.initial.width.lerp(self.target.width, amount);
            }
            if self.active.intersects(TransitionProperties::HEIGHT) {
                self.current.height = self.initial.height.lerp(self.target.height, amount);
            }
            if progress == 1.0 {
                self.current = self.target;
                self.started_at = None;
                self.active = TransitionProperties::NONE;
            }
        }

        let mut changed = TransitionProperties::NONE;
        if self.config.properties.intersects(TransitionProperties::X) && self.target.x != target.x {
            changed = changed.union(TransitionProperties::X);
        }
        if self.config.properties.intersects(TransitionProperties::Y) && self.target.y != target.y {
            changed = changed.union(TransitionProperties::Y);
        }
        if self.config.properties.intersects(TransitionProperties::WIDTH) && self.target.width != target.width {
            changed = changed.union(TransitionProperties::WIDTH);
        }
        if self.config.properties.intersects(TransitionProperties::HEIGHT) && self.target.height != target.height {
            changed = changed.union(TransitionProperties::HEIGHT);
        }

        self.target = target;
        if !changed.is_empty() {
            self.initial = self.current;
            self.active = self.active.union(changed);
            if self.config.duration.is_zero() {
                self.current = target;
                self.started_at = None;
                self.active = TransitionProperties::NONE;
            } else {
                self.started_at = Some(now);
            }
        } else if self.started_at.is_none() {
            self.initial = target;
            self.current = target;
        }
    }
}
