use std::time::Duration;

use super::{Frame, NodeId, Sizing, position};
use crate::{
    Platform,
    animation::{Transition, TransitionProperties},
    geometry::{Rect, Size},
    interact::WidgetId,
    layout::Axis,
};

pub fn resolve<R: Platform>(frame: &mut Frame<R>, platform: &mut R, size: Size) {
    for index in 0..frame.geometry.len() {
        let record = frame.geometry[index];
        let (Some(id), Some(config)) = (record.id, record.transition) else {
            continue;
        };
        match frame
            .transitions
            .binary_search_by_key(&id, |state| state.id)
        {
            Ok(index) => frame.transitions[index].begin(record.node, config),
            Err(index) => frame
                .transitions
                .insert(index, TransitionState::new(id, record.node, config)),
        }
    }

    position::layout(frame, platform, size);
    let mut active = TransitionProperties::NONE;
    for index in 0..frame.transitions.len() {
        if !frame.transitions[index].seen {
            continue;
        }
        let node = frame.transitions[index].node;
        let mut target = frame.nodes[node.index()].area;
        let offset = position::offset(frame, node);
        target.x -= offset.x;
        target.y -= offset.y;
        frame.transitions[index].advance(target, frame.time);
        active = active.union(frame.transitions[index].active);
    }

    if active.intersects(TransitionProperties::SIZE) {
        for state in frame.transitions.iter().filter(|state| state.seen) {
            let node = &mut frame.nodes[state.node.index()];
            if state.active.intersects(TransitionProperties::WIDTH) {
                node.place.width = frame
                    .layout_resolution
                    .sizing(Axis::Horizontal, Sizing::fixed(state.current.width));
            }
            if state.active.intersects(TransitionProperties::HEIGHT) {
                node.place.height = frame
                    .layout_resolution
                    .sizing(Axis::Vertical, Sizing::fixed(state.current.height));
            }
        }
        position::layout(frame, platform, size);
    }

    if active.intersects(TransitionProperties::POSITION) {
        for state in frame.transitions.iter().filter(|state| state.seen) {
            let offset = position::offset(frame, state.node);
            let area = &mut frame.nodes[state.node.index()].area;
            if state.active.intersects(TransitionProperties::X) {
                area.x = state.current.x + offset.x;
            }
            if state.active.intersects(TransitionProperties::Y) {
                area.y = state.current.y + offset.y;
            }
        }
    }
}

pub struct TransitionState {
    pub id: WidgetId,
    pub current: Rect,
    pub initial: Rect,
    pub target: Rect,
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

    pub fn begin(&mut self, node: NodeId, config: Transition) {
        assert!(!self.seen, "duplicate transition WidgetId {:?}", self.id);
        self.node = node;
        self.config = config;
        self.seen = true;
    }

    pub fn is_active(&self) -> bool {
        self.started_at.is_some()
    }

    pub fn advance(&mut self, target: Rect, now: Duration) {
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
