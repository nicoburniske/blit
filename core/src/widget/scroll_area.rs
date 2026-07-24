use std::time::Duration;

use super::SizedWidget;
use crate::{
    geometry::{LogicalInsets, LogicalRect, PhysicalRect},
    input::ScrollPhase,
    interact::{Sense, WidgetId},
    Ui,
};

const WHEEL_FRICTION: f32 = 64.0;
const MIN_SCROLL_VELOCITY: f32 = 5.0;
const MAX_SCROLL_VELOCITY: f32 = 12_000.0;
const MAX_FRAME_TIME: f32 = 0.05;

#[derive(Debug)]
pub struct ScrollState {
    pub offset: f32,
    pub content_height: f32,
    pub id: WidgetId,
    velocity: f32,
    tracking: bool,
    continuous_inertia: bool,
    last_frame: Option<Duration>,
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
        }
    }
}

impl ScrollState {
    pub fn maximum_offset(&self, viewport_height: f32) -> f32 {
        (self.content_height - viewport_height).max(0.0)
    }

    pub fn scroll_by(&mut self, pixels: f32, viewport_height: f32) {
        self.offset = (self.offset + pixels).clamp(0.0, self.maximum_offset(viewport_height));
        self.velocity = 0.0;
        self.tracking = false;
    }

    pub fn scroll_to(&mut self, offset: f32, viewport_height: f32) {
        self.offset = offset.clamp(0.0, self.maximum_offset(viewport_height));
        self.velocity = 0.0;
        self.tracking = false;
    }

    pub fn is_moving(&self) -> bool { self.velocity != 0.0 }
}

pub struct ScrollArea<'a> {
    state: &'a mut ScrollState,
    spacing: f32,
    padding: LogicalInsets,
    scroll_speed: f32,
    inertia_friction: f32,
    drag_to_scroll: bool,
    id: WidgetId,
}

impl<'a> ScrollArea<'a> {
    pub fn vertical(state: &'a mut ScrollState) -> Self {
        let id = state.id;
        Self {
            state,
            spacing: 0.0,
            padding: LogicalInsets::default(),
            scroll_speed: 1.0,
            inertia_friction: 6.0,
            drag_to_scroll: true,
            id,
        }
    }

    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing.max(0.0);
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

    pub fn begin<'ui>(self, ui: &'ui mut Ui, viewport: LogicalRect) -> Area<'ui>
    where
        'a: 'ui,
    {
        self.begin_with_padding(ui, viewport, |_, padding| padding)
    }

    pub fn begin_with_padding<'ui>(
        self,
        ui: &'ui mut Ui,
        viewport: LogicalRect,
        padding: impl FnOnce(f32, LogicalInsets) -> LogicalInsets,
    ) -> Area<'ui>
    where
        'a: 'ui,
    {
        let sense = if self.drag_to_scroll { Sense::SCROLL_AND_DRAG } else { Sense::SCROLL };
        let interaction = ui.interact(ui.id(("scroll area", self.id)), viewport, sense);
        let now = ui.time();
        let elapsed = self
            .state
            .last_frame
            .replace(now)
            .map_or(0.0, |previous| now.saturating_sub(previous).as_secs_f32().min(MAX_FRAME_TIME));

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
        } else if interaction.drag_released {
            self.state.tracking = false;
            released = true;
        } else if let Some(phase) = interaction.scroll_phase {
            if interaction.scroll_continuous {
                match phase {
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
                direct_delta = interaction.scroll_delta.y * self.scroll_speed;
                self.state.continuous_inertia = true;
            } else if interaction.scroll_delta.y != 0.0 {
                self.state.tracking = false;
                self.state.continuous_inertia = false;
                self.state.velocity += interaction.scroll_delta.y * WHEEL_FRICTION * self.scroll_speed;
            }
        }

        let maximum = self.state.maximum_offset(viewport.height);
        if direct_delta != 0.0 {
            self.state.offset = (self.state.offset + direct_delta).clamp(0.0, maximum);
            if sample_velocity && elapsed > 0.0 {
                let measured = (direct_delta / elapsed).clamp(-MAX_SCROLL_VELOCITY, MAX_SCROLL_VELOCITY);
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
            let friction = if self.state.continuous_inertia { self.inertia_friction } else { WHEEL_FRICTION };
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

        let padding = padding(self.state.offset, self.padding);
        let bounds = viewport.inset(padding);
        let offset = self.state.offset;
        let previous_clip = ui.clip;
        ui.clip = viewport.to_physical(ui.scale_factor).intersection(previous_clip).unwrap_or_default();

        Area {
            ui,
            state: self.state,
            viewport,
            bounds,
            padding,
            offset,
            spacing: self.spacing,
            cursor: 0.0,
            count: 0,
            previous_clip,
        }
    }
}

pub struct Area<'a> {
    ui: &'a mut Ui,
    state: &'a mut ScrollState,
    viewport: LogicalRect,
    bounds: LogicalRect,
    padding: LogicalInsets,
    offset: f32,
    spacing: f32,
    cursor: f32,
    count: usize,
    previous_clip: PhysicalRect,
}

impl Area<'_> {
    pub fn add<W: SizedWidget>(&mut self, widget: W) -> Option<W::Output> {
        let available = LogicalRect {
            x: self.bounds.x,
            y: self.bounds.y + self.cursor - self.offset,
            width: self.bounds.width,
            height: f32::INFINITY,
        };
        let size = widget.measure(self.ui, available);
        let area =
            LogicalRect { width: size.width.clamp(0.0, available.width), height: size.height, ..available };
        self.cursor += area.height + self.spacing;
        self.count += 1;
        area.to_physical(self.ui.scale_factor)
            .intersection(self.ui.clip)
            .map(|_| widget.render(self.ui, area))
    }

    pub fn ui(&mut self) -> &mut Ui { self.ui }

    pub fn finish(self) { drop(self) }
}

impl Drop for Area<'_> {
    fn drop(&mut self) {
        let used_height = if self.count == 0 { 0.0 } else { self.cursor - self.spacing };
        self.state.content_height = self.padding.top + used_height + self.padding.bottom;
        let maximum = self.state.maximum_offset(self.viewport.height);
        let clamped = self.state.offset.clamp(0.0, maximum);
        if clamped != self.state.offset {
            self.state.offset = clamped;
            self.state.velocity = 0.0;
            self.ui.request_frame();
        }
        self.ui.clip = self.previous_clip;
    }
}
