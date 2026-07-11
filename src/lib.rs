mod animation;
mod color;
mod component;
mod image;
mod interaction;
mod keyboard;
mod layout;
mod platform;
mod rect;
#[cfg(test)]
mod test;
mod text;
pub mod widgets;

use std::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
    time::{Duration, Instant},
};

pub use animation::Easing;
pub use color::Color;
pub use component::SizedComponent;
pub use image::{ImageData, ImageFormat, ImageId, ImagePixels, ImageResource};
pub use interaction::{Interaction, Sense, WidgetId};
pub use keyboard::{KeyboardKind, KeyboardRequest};
pub use layout::{Constraint, Direction, Layout, RepeatedAreas, RepeatedLayout};
pub use platform::{Platform, PlatformImpl, PlatformVTable};
pub use rect::{LogicalInsets, LogicalRect, LogicalSize, PhysicalRect, PhysicalSize, Size};
pub use text::{
    FontId, FontWeight, HorizontalAlign, LogicalPoint, PhysicalPoint, Text, TextOptions,
    TextOverflow, TextRequest, TextStyle, TextWrap, VerticalAlign,
};

#[derive(Clone, Debug, Default)]
pub struct DirtyRegions {
    regions: [PhysicalRect; 8],
    len: usize,
}

impl DirtyRegions {
    pub fn add(&mut self, mut area: PhysicalRect) {
        if area.width <= 0 || area.height <= 0 {
            return;
        }

        loop {
            let mut index = 0;
            while index < self.len {
                if area.touches(self.regions[index]) {
                    area = area.union(self.regions[index]);
                    self.len -= 1;
                    self.regions[index] = self.regions[self.len];
                    index = 0;
                } else {
                    index += 1;
                }
            }

            if self.len < self.regions.len() {
                self.regions[self.len] = area;
                self.len += 1;
                return;
            }

            let mut best = 0;
            let mut growth = i64::MAX;
            for index in 0..self.len {
                let candidate = area.union(self.regions[index]);
                let candidate_growth = candidate.area() - self.regions[index].area();
                if candidate_growth < growth {
                    best = index;
                    growth = candidate_growth;
                }
            }
            area = area.union(self.regions[best]);
            self.len -= 1;
            self.regions[best] = self.regions[self.len];
        }
    }

    pub fn regions(&self) -> &[PhysicalRect] {
        &self.regions[..self.len]
    }

    pub fn extend(&mut self, other: &Self) {
        for area in other.regions() {
            self.add(*area);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Input {
    #[default]
    None,
    PointerDown {
        position: LogicalPoint,
    },
    PointerUp {
        position: LogicalPoint,
        leave: bool,
    },
    PointerMove {
        position: LogicalPoint,
    },
    PointerLeave,
    Scroll {
        position: LogicalPoint,
        delta_x: f32,
        delta_y: f32,
    },
    Char(char),
    Backspace,
    Delete,
    CursorLeft,
    CursorRight,
    Enter,
    Tab,
}

pub struct Runtime {
    started_at: Instant,
    platform: Platform,
    screen: LogicalRect,
    physical_screen: PhysicalRect,
    scale_factor: f32,
    pending: DirtyRegions,
    previous: DirtyRegions,
    interaction: interaction::InteractionState,
    animations: Vec<animation::AnimationState>,
}

impl Runtime {
    pub fn new(mut platform: Platform) -> Self {
        let physical_screen = platform.screen();
        let scale_factor = platform.scale_factor();
        assert!(scale_factor.is_finite() && scale_factor > 0.0);
        let screen = physical_screen.to_logical(scale_factor);
        let mut pending = DirtyRegions::default();
        pending.add(physical_screen);
        Self {
            started_at: Instant::now(),
            platform,
            screen,
            physical_screen,
            scale_factor,
            pending,
            previous: DirtyRegions::default(),
            interaction: interaction::InteractionState::default(),
            animations: Vec::new(),
        }
    }

    pub fn render<R>(&mut self, input: Input, render: impl FnOnce(&mut Ui) -> R) -> R {
        self.render_at(self.started_at.elapsed(), input, render)
    }

    fn render_at<R>(
        &mut self,
        time: Duration,
        input: Input,
        render: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        let pending = std::mem::take(&mut self.pending);
        let mut dirty = std::mem::take(&mut self.previous);
        dirty.extend(&pending);
        self.previous = pending;
        self.platform.begin_frame();
        let mut interaction = std::mem::take(&mut self.interaction);
        let mut animations = std::mem::take(&mut self.animations);
        for animation in &mut animations {
            animation.begin_frame();
        }
        let hover_damage = interaction.begin_frame(&input, self.scale_factor);
        let mut invalidated = DirtyRegions::default();
        for area in hover_damage.into_iter().flatten() {
            invalidated.add(area);
        }
        let mut ui = Ui {
            platform: NonNull::from(&mut self.platform),
            time,
            input,
            screen: self.screen,
            clip: self.physical_screen,
            scale_factor: self.scale_factor,
            dirty,
            invalidated,
            current_id: WidgetId::new("blit root"),
            interaction,
            animations,
            animation_stack: [0; 8],
            animation_depth: 0,
        };
        let output = render(&mut ui);
        assert_eq!(ui.animation_depth, 0, "animation scope was not dropped");
        for area in ui
            .interaction
            .end_frame(self.scale_factor)
            .into_iter()
            .flatten()
        {
            ui.invalidated.add(area);
        }
        let mut index = 0;
        while index < ui.animations.len() {
            if ui.animations[index].seen {
                index += 1;
            } else {
                if let Some(area) = ui.animations[index].previous_bounds {
                    ui.invalidated.add(area);
                }
                ui.animations.swap_remove(index);
            }
        }
        self.pending = ui.invalidated;
        self.interaction = ui.interaction;
        self.animations = ui.animations;
        self.platform.end_frame();
        output
    }

    pub fn has_pending_redraw(&self) -> bool {
        !self.pending.is_empty()
            || !self.previous.is_empty()
            || self.animations.iter().any(|animation| animation.active)
    }

    pub fn invalidate(&mut self, area: LogicalRect) {
        if let Some(area) = area
            .to_physical(self.scale_factor)
            .intersection(self.physical_screen)
        {
            self.pending.add(area)
        }
    }

    pub fn invalidate_all(&mut self) {
        self.pending.add(self.physical_screen)
    }

    pub fn screen(&self) -> LogicalRect {
        self.screen
    }
}

pub struct Ui {
    platform: NonNull<Platform>,
    time: Duration,
    input: Input,
    screen: LogicalRect,
    clip: PhysicalRect,
    scale_factor: f32,
    dirty: DirtyRegions,
    invalidated: DirtyRegions,
    current_id: WidgetId,
    interaction: interaction::InteractionState,
    animations: Vec<animation::AnimationState>,
    animation_stack: [usize; 8],
    animation_depth: usize,
}

impl Ui {
    pub fn platform(&mut self) -> &mut Platform {
        unsafe { self.platform.as_mut() }
    }

    pub fn screen(&self) -> LogicalRect {
        self.screen
    }

    pub fn input(&self) -> &Input {
        &self.input
    }

    pub fn time(&self) -> Duration {
        self.time
    }

    pub fn animate(
        &mut self,
        id: WidgetId,
        target: f32,
        duration: Duration,
        easing: Easing,
    ) -> AnimationScope<'_> {
        assert!(
            self.animation_depth < self.animation_stack.len(),
            "animation scopes nested too deeply"
        );
        let index = self
            .animations
            .iter()
            .position(|animation| animation.id == id)
            .unwrap_or_else(|| {
                self.animations
                    .push(animation::AnimationState::new(id, target));
                self.animations.len() - 1
            });
        assert!(
            !self.animations[index].seen,
            "duplicate animation WidgetId {id:?}"
        );
        self.animations[index].advance(target, duration, easing, self.time);
        if self.animations[index].damage {
            if let Some(area) = self.animations[index]
                .previous_bounds
                .and_then(|area| area.intersection(self.clip))
            {
                self.dirty.add(area);
            }
        }
        self.animation_stack[self.animation_depth] = index;
        self.animation_depth += 1;
        AnimationScope { ui: self, index }
    }

    pub fn id(&self, source: impl std::hash::Hash) -> WidgetId {
        self.current_id.child(source)
    }

    pub fn begin_scope(&mut self, source: impl std::hash::Hash) -> IdScope<'_> {
        let previous = self.current_id;
        self.current_id = self.current_id.child(source);
        IdScope { ui: self, previous }
    }

    pub fn interact(&mut self, id: WidgetId, area: LogicalRect, sense: Sense) -> Interaction {
        let area = area.to_physical(self.scale_factor).intersection(self.clip);
        self.interaction.interact(id, area, sense)
    }

    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.interaction.is_focused(id)
    }

    pub fn focus(&mut self, id: WidgetId) {
        for area in self.interaction.focus(id).into_iter().flatten() {
            self.invalidated.add(area);
        }
    }

    pub fn clear_focus(&mut self) {
        if let Some(area) = self.interaction.clear_focus() {
            self.invalidated.add(area);
        }
    }

    pub fn pointer_position(&self) -> Option<LogicalPoint> {
        self.interaction.pointer_position()
    }

    pub fn invalidate(&mut self, area: LogicalRect) {
        if let Some(area) = area.to_physical(self.scale_factor).intersection(self.clip) {
            self.invalidated.add(area)
        }
    }

    pub fn invalidate_all(&mut self) {
        self.invalidated.add(self.clip)
    }

    pub(crate) fn record_draw(&mut self, area: LogicalRect) {
        let Some(area) = area.to_physical(self.scale_factor).intersection(self.clip) else {
            return;
        };
        for depth in 0..self.animation_depth {
            let index = self.animation_stack[depth];
            let animation = &mut self.animations[index];
            animation.current_bounds = Some(
                animation
                    .current_bounds
                    .map_or(area, |bounds| bounds.union(area)),
            );
            if animation.damage {
                self.dirty.add(area);
            }
        }
    }
}

pub struct AnimationScope<'a> {
    ui: &'a mut Ui,
    index: usize,
}

impl AnimationScope<'_> {
    pub fn value(&self) -> f32 {
        self.ui.animations[self.index].value
    }

    pub fn is_active(&self) -> bool {
        self.ui.animations[self.index].active
    }

    pub fn ui(&mut self) -> &mut Ui {
        self.ui
    }

    pub fn finish(self) {
        drop(self)
    }
}

impl Deref for AnimationScope<'_> {
    type Target = Ui;

    fn deref(&self) -> &Self::Target {
        self.ui
    }
}

impl DerefMut for AnimationScope<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ui
    }
}

impl Drop for AnimationScope<'_> {
    fn drop(&mut self) {
        assert_eq!(
            self.ui.animation_stack[self.ui.animation_depth - 1],
            self.index,
            "animation scopes must be dropped in reverse order"
        );
        self.ui.animation_depth -= 1;
        let animation = &mut self.ui.animations[self.index];
        if animation.changed && !animation.active {
            if let Some(area) = animation.previous_bounds {
                self.ui.invalidated.add(area);
            }
            if let Some(area) = animation.current_bounds {
                self.ui.invalidated.add(area);
            }
        }
        if animation.active {
            if let Some(area) = animation.current_bounds {
                self.ui.invalidated.add(area);
            }
        }
        animation.previous_bounds = animation.current_bounds;
    }
}

pub struct IdScope<'a> {
    ui: &'a mut Ui,
    previous: WidgetId,
}

impl IdScope<'_> {
    pub fn ui(&mut self) -> &mut Ui {
        self.ui
    }

    pub fn finish(self) {
        drop(self)
    }
}

impl Drop for IdScope<'_> {
    fn drop(&mut self) {
        self.ui.current_id = self.previous;
    }
}
