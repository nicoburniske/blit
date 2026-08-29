//! frame layout transitions

use super::*;

pub struct TransitionState {
    pub id: WidgetId,
    pub current: LogicalRect,
    pub initial: LogicalRect,
    pub target: LogicalRect,
    pub started_at: Option<Duration>,
    pub active: TransitionProperties,
    pub node: NodeId,
    pub config: Transition,
    pub initialized: bool,
    pub seen: bool,
}

impl TransitionState {
    pub fn new(id: WidgetId, node: NodeId, config: Transition) -> Self {
        Self {
            id,
            current: LogicalRect::default(),
            initial: LogicalRect::default(),
            target: LogicalRect::default(),
            started_at: None,
            active: TransitionProperties::NONE,
            node,
            config,
            initialized: false,
            seen: true,
        }
    }

    pub fn begin(&mut self, node: NodeId, config: Transition) {
        assert!(!self.seen, "duplicate transition WidgetId {:?}", self.id);
        self.node = node;
        self.config = config;
        self.seen = true;
    }

    pub fn is_active(&self) -> bool {
        self.started_at.is_some()
    }

    pub fn advance(&mut self, target: LogicalRect, now: Duration) {
        if !self.initialized {
            self.current = target;
            self.initial = target;
            self.target = target;
            self.initialized = true;
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
            let progress = (now.saturating_sub(started_at).as_secs_f32()
                / self.config.duration.as_secs_f32())
            .min(1.0);
            let amount = self.config.easing.apply(progress);
            if self.active.intersects(TransitionProperties::X) {
                self.current.x = self.initial.x + (self.target.x - self.initial.x) * amount;
            }
            if self.active.intersects(TransitionProperties::Y) {
                self.current.y = self.initial.y + (self.target.y - self.initial.y) * amount;
            }
            if self.active.intersects(TransitionProperties::WIDTH) {
                self.current.width =
                    self.initial.width + (self.target.width - self.initial.width) * amount;
            }
            if self.active.intersects(TransitionProperties::HEIGHT) {
                self.current.height =
                    self.initial.height + (self.target.height - self.initial.height) * amount;
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
        if self
            .config
            .properties
            .intersects(TransitionProperties::WIDTH)
            && self.target.width != target.width
        {
            changed = changed.union(TransitionProperties::WIDTH);
        }
        if self
            .config
            .properties
            .intersects(TransitionProperties::HEIGHT)
            && self.target.height != target.height
        {
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
