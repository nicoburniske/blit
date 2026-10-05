use std::time::Duration;

use super::{Frame, NodeId, layout};
use crate::{
    Context, Scalar,
    animation::{Transition, TransitionProperties},
    arena::DataArena,
    geometry::{Rect, Size},
    interact::WidgetId,
};

/// resolves transitions against the frame's target layout
///
/// - layout first establishes target geometry
/// - active size transitions write animated sizes into node geometry and replay layout
/// - target sizes remain available for structural decisions such as wrapping
/// - position transitions apply after layout without replay
pub fn resolve<C: Context>(
    frame: &mut Frame<C>,
    data: &DataArena,
    context: &mut C,
    size: Size<C::Scalar>,
    resized: bool,
) {
    for index in 0..frame.geometry.len() {
        let record = frame.geometry[index];
        let Some(config) = record.transition else {
            continue;
        };
        let id = frame.nodes[record.node.index()].widget_id;
        match frame.transitions.binary_search_by_key(&id, |state| state.id) {
            Ok(index) => frame.transitions[index].begin(record.node, config),
            Err(index) => frame
                .transitions
                .insert(index, TransitionState::new(id, record.node, config)),
        }
    }

    layout::resolve(frame, data, context, size);
    let mut active = TransitionProperties::NONE;
    for index in 0..frame.transitions.len() {
        if !frame.transitions[index].seen {
            continue;
        }
        let node = frame.transitions[index].node;
        let target = frame.node_geometry[node.index()].area;
        frame.transitions[index].advance(target, frame.time, resized);
        active = active.union(frame.transitions[index].active);
    }

    if active.intersects(TransitionProperties::SIZE) {
        frame
            .target_sizes
            .extend(frame.node_geometry.iter().map(|node| super::TargetSize {
                size: node.area.size(),
                properties: TransitionProperties::NONE,
            }));
        let mut relayout = false;
        for state in &mut frame.transitions {
            if !state.seen {
                continue;
            }
            let node = state.node;
            let properties = state.active.intersection(TransitionProperties::SIZE);
            if properties.is_empty() {
                continue;
            }
            let parent = frame.nodes[node.index()].parent;
            if parent == node {
                if properties.intersects(TransitionProperties::WIDTH) {
                    state.current.width = state.target.width;
                    state.initial.width = state.target.width;
                }
                if properties.intersects(TransitionProperties::HEIGHT) {
                    state.current.height = state.target.height;
                    state.initial.height = state.target.height;
                }
                state.active = state.active.intersection(TransitionProperties::POSITION);
                if state.active.is_empty() {
                    state.started_at = None;
                }
                continue;
            }
            let area = &mut frame.node_geometry[node.index()].area;
            if properties.intersects(TransitionProperties::WIDTH) {
                area.width = state.current.width;
            }
            if properties.intersects(TransitionProperties::HEIGHT) {
                area.height = state.current.height;
            }
            frame.target_sizes[node.index()].properties = properties;
            relayout = true;
        }
        if relayout {
            layout::resolve(frame, data, context, size);
        }
        frame.target_sizes.clear();
    }

    if active.intersects(TransitionProperties::POSITION) {
        for state in frame.transitions.iter().filter(|state| state.seen) {
            let area = &mut frame.node_geometry[state.node.index()].area;
            if state.active.intersects(TransitionProperties::X) {
                area.x = state.current.x;
            }
            if state.active.intersects(TransitionProperties::Y) {
                area.y = state.current.y;
            }
        }
    }
}

pub struct TransitionState<T> {
    pub id: WidgetId,
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
        self.node = node;
        self.config = config;
        self.seen = true;
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
