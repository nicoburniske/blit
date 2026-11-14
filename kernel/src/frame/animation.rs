use super::*;

pub struct AnimationState {
    pub id: Id,
    pub playback: Playback,
    pub seen: bool,
}

impl AnimationState {
    pub fn update(animations: &mut Vec<Self>, id: Id, initial: f32) -> &mut Self {
        let index = match animations.binary_search_by_key(&id, |animation| animation.id) {
            Ok(index) => index,
            Err(index) => {
                animations.insert(
                    index,
                    Self {
                        id,
                        playback: Playback::Target(Animator::new(
                            [initial],
                            Animation::default(),
                            AnimationOptions::default(),
                        )),
                        seen: false,
                    },
                );
                index
            }
        };
        let animation = &mut animations[index];
        assert!(!animation.seen, "duplicate animation Id {id:?}");
        animation.seen = true;
        animation
    }

    pub fn is_active(&self) -> bool {
        match &self.playback {
            Playback::Target(animator) => animator.is_active(),
            Playback::Loop { .. } => true,
        }
    }
}

pub enum Playback {
    Target(Animator<f32, 1>),
    Loop { tween: Tween, time: f64, value: f32 },
}

pub struct MotionState<T: Scalar> {
    pub id: Id,
    pub animator: Animator<T, 4>,
    pub properties: MotionProps,
    pub node: NodeId,
    pub initialized: bool,
    pub seen: bool,
}

impl<T: Scalar> MotionState<T> {
    pub fn new(id: Id, node: NodeId) -> Self {
        Self {
            id,
            animator: Animator::new([T::ZERO; 4], Animation::default(), AnimationOptions::default()),
            properties: MotionProps::NONE,
            node,
            initialized: false,
            seen: true,
        }
    }

    pub fn is_active(&self) -> bool {
        self.animator.is_active()
    }

    pub fn advance(
        &mut self,
        target: Rect<T>,
        config: Motion,
        now: Duration,
        resized: bool,
        commit: bool,
    ) -> ([T; 4], MotionProps) {
        let targets = [target.x, target.y, target.width, target.height];
        let options = AnimationOptions {
            initial: None,
            restart: config.restart,
            velocity: None,
        };
        if !self.initialized || (resized && !config.properties.intersects(MotionProps::RESIZE)) {
            if commit {
                self.animator = Animator::new(targets, config.animation, options);
                self.initialized = true;
            }
            return (targets, MotionProps::NONE);
        }
        let properties = [MotionProps::X, MotionProps::Y, MotionProps::WIDTH, MotionProps::HEIGHT];
        let (mut values, active) = self.animator.advance(
            targets,
            properties.map(|p| config.properties.intersects(p)),
            config.animation,
            options,
            now,
            commit,
        );
        for (i, value) in values.iter_mut().enumerate().skip(2) {
            if *value < T::ZERO {
                *value = T::ZERO;
                if commit {
                    self.animator.clamp(i, T::ZERO, T::UNBOUNDED, now);
                }
            }
        }
        let mut animated = MotionProps::NONE;
        for (i, property) in properties.into_iter().enumerate() {
            if active[i] {
                animated |= property;
            }
        }
        (values, animated)
    }
}
