use std::time::Duration;

use crate::{
    Ui,
    container::Sizing,
    geometry::{LogicalInsets, LogicalPoint},
    input::ScrollPhase,
    interact::{Sense, WidgetId},
    layout::Flex,
    node::NodeId,
    style::Clip,
};

pub struct ScrollArea<'a> {
    state: &'a mut ScrollState,
    width: Sizing,
    height: Sizing,
    z_index: i16,
    gap: f32,
    padding: LogicalInsets,
    scroll_speed: f32,
    inertia_friction: f32,
    drag_to_scroll: bool,
    id: WidgetId,
}

#[derive(Debug)]
pub struct ScrollState {
    pub offset: f32,
    pub content_height: f32,
    pub id: WidgetId,
    velocity: f32,
    tracking: bool,
    continuous_inertia: bool,
    last_frame: Option<Duration>,
    viewport_height: f32,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            offset: 0.0,
            content_height: 0.0,
            id: WidgetId::unique(),
            velocity: 0.0,
            tracking: false,
            continuous_inertia: false,
            last_frame: None,
            viewport_height: 0.0,
        }
    }
}

impl ScrollState {
    pub fn maximum_offset(&self) -> f32 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    pub fn scroll_by(&mut self, pixels: f32) {
        self.offset = (self.offset + pixels).clamp(0.0, self.maximum_offset());
        self.velocity = 0.0;
        self.tracking = false;
    }

    pub fn scroll_to(&mut self, offset: f32) {
        self.offset = offset.clamp(0.0, self.maximum_offset());
        self.velocity = 0.0;
        self.tracking = false;
    }

    pub fn is_moving(&self) -> bool {
        self.velocity != 0.0
    }
}

pub struct ScrollScope<'a> {
    ui: &'a mut Ui,
    viewport: NodeId,
    content: NodeId,
}

impl<'a> ScrollArea<'a> {
    pub fn vertical(state: &'a mut ScrollState) -> Self {
        let id = state.id;
        Self {
            state,
            width: Sizing::grow(),
            height: Sizing::grow(),
            z_index: 0,
            gap: 0.0,
            padding: LogicalInsets::default(),
            scroll_speed: 1.0,
            inertia_friction: 6.0,
            drag_to_scroll: true,
            id,
        }
    }

    pub fn width(mut self, width: Sizing) -> Self {
        self.width = width;
        self
    }

    pub fn height(mut self, height: Sizing) -> Self {
        self.height = height;
        self
    }

    pub fn z_index(mut self, z_index: i16) -> Self {
        self.z_index = z_index;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap.max(0.0);
        self
    }

    pub fn padding(mut self, padding: LogicalInsets) -> Self {
        self.padding = padding;
        self
    }

    pub fn scroll_speed(mut self, speed: f32) -> Self {
        self.scroll_speed = speed.max(0.0);
        self
    }

    pub fn inertia_friction(mut self, friction: f32) -> Self {
        self.inertia_friction = friction.max(f32::EPSILON);
        self
    }

    pub fn drag_to_scroll(mut self, enabled: bool) -> Self {
        self.drag_to_scroll = enabled;
        self
    }

    pub fn id(mut self, source: impl std::hash::Hash) -> Self {
        self.id = WidgetId::new(source);
        self
    }

    pub fn begin(self, ui: &'a mut Ui) -> ScrollScope<'a> {
        let id = WidgetId::new(("scroll area", self.id));
        let content_id = id.child("content");
        self.state.viewport_height = ui.geometry(id).map_or(0.0, |area| area.height);
        if let Some(area) = ui.geometry(content_id) {
            self.state.content_height = area.height;
        }

        let sense = if self.drag_to_scroll {
            Sense::SCROLL_AND_DRAG
        } else {
            Sense::SCROLL
        };
        let mut viewport_config = crate::ContainerConfig::default();
        viewport_config.item.width = self.width;
        viewport_config.item.height = self.height;
        viewport_config.item.z_index = self.z_index;
        viewport_config.id = Some(id);
        viewport_config.clip = Clip::Bounds;
        let interaction = ui.interact(id, sense);
        let viewport = ui.open_layout(Flex::column().overflow(true), viewport_config);
        let now = ui.time();
        let elapsed = self.state.last_frame.replace(now).map_or(0.0, |previous| {
            now.saturating_sub(previous)
                .as_secs_f32()
                .min(MAX_FRAME_TIME)
        });

        let mut direct_delta = 0.0;
        let mut sample_velocity = false;
        let mut released = false;
        if interaction.drag_delta.y != 0.0 {
            direct_delta = -interaction.drag_delta.y * self.scroll_speed;
            sample_velocity = self.state.tracking;
            if !self.state.tracking {
                self.state.velocity = 0.0;
            }
            self.state.tracking = true;
            self.state.continuous_inertia = true;
        } else if interaction.deactivated {
            self.state.tracking = false;
            released = true;
        } else if let Some(scroll) = interaction.scroll {
            if scroll.continuous {
                match scroll.phase {
                    ScrollPhase::Started => {
                        self.state.velocity = 0.0;
                        self.state.tracking = true;
                    }
                    ScrollPhase::Moved => {
                        sample_velocity = self.state.tracking;
                        self.state.tracking = true;
                    }
                    ScrollPhase::Ended => {
                        self.state.tracking = false;
                        released = true;
                    }
                }
                direct_delta = scroll.delta.y * self.scroll_speed;
                self.state.continuous_inertia = true;
            } else if scroll.delta.y != 0.0 {
                self.state.tracking = false;
                self.state.continuous_inertia = false;
                self.state.velocity += scroll.delta.y * WHEEL_FRICTION * self.scroll_speed;
            }
        }

        let maximum = self.state.maximum_offset();
        if direct_delta != 0.0 {
            self.state.offset = (self.state.offset + direct_delta).clamp(0.0, maximum);
            if sample_velocity && elapsed > 0.0 {
                let measured =
                    (direct_delta / elapsed).clamp(-MAX_SCROLL_VELOCITY, MAX_SCROLL_VELOCITY);
                self.state.velocity = if self.state.velocity.signum() == measured.signum() {
                    self.state.velocity + (measured - self.state.velocity) * 0.5
                } else {
                    measured
                };
            }
        }

        if released && self.state.velocity.abs() < MIN_SCROLL_VELOCITY {
            self.state.velocity = 0.0;
        }

        if !self.state.tracking && self.state.velocity != 0.0 {
            let friction = if self.state.continuous_inertia {
                self.inertia_friction
            } else {
                WHEEL_FRICTION
            };
            let decay = (-friction * elapsed).exp();
            let offset = self.state.offset + self.state.velocity * (1.0 - decay) / friction;
            self.state.offset = offset.clamp(0.0, maximum);
            self.state.velocity *= decay;
            if self.state.offset != offset || self.state.velocity.abs() < MIN_SCROLL_VELOCITY {
                self.state.velocity = 0.0;
            } else {
                ui.request_frame();
            }
        } else {
            self.state.offset = self.state.offset.clamp(0.0, maximum);
        }

        let mut content_config = crate::ContainerConfig::default();
        content_config.item.width = Sizing::grow();
        content_config.offset = LogicalPoint {
            x: 0.0,
            y: -self.state.offset,
        };
        content_config.id = Some(content_id);
        let content = ui.open_layout(
            Flex::column().padding(self.padding).gap(self.gap),
            content_config,
        );
        ScrollScope {
            ui,
            viewport,
            content,
        }
    }
}

impl ScrollScope<'_> {
    pub fn add<W: super::Widget>(&mut self, widget: W) -> W::Output {
        widget.render(self.ui)
    }
}

impl Drop for ScrollScope<'_> {
    fn drop(&mut self) {
        self.ui.close_container(self.content);
        self.ui.close_container(self.viewport);
    }
}

const WHEEL_FRICTION: f32 = 64.0;
const MIN_SCROLL_VELOCITY: f32 = 5.0;
const MAX_SCROLL_VELOCITY: f32 = 12_000.0;
const MAX_FRAME_TIME: f32 = 0.05;
