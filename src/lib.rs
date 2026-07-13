mod animation;
mod color;
mod component;
mod dirty;
mod image;
mod input;
mod interaction;
mod keyboard;
mod layout;
mod platform;
mod rect;
#[cfg(test)]
mod test;
mod text;
mod timer;
pub mod widgets;

use std::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
    time::Duration,
};

pub use animation::Easing;
pub use color::Color;
pub use component::SizedComponent;
pub use dirty::DirtyRegions;
pub use image::{ImageData, ImageFormat, ImageHandle, ImageId, ImagePixels};
pub use input::Input;
pub use interaction::{Interaction, Sense, WidgetId};
pub use keyboard::{KeyboardKind, KeyboardRequest};
pub use layout::{Constraint, Direction, Layout, LayoutAlign, RepeatedAreas, RepeatedLayout};
pub use platform::{Platform, PlatformImpl};
pub use rect::{LogicalInsets, LogicalRect, LogicalSize, PhysicalRect, PhysicalSize, Size};
pub use text::{
    FontId, HorizontalAlign, LogicalPoint, PhysicalPoint, Text, TextOptions, TextOverflow,
    TextRequest, TextStyle, TextWrap, VerticalAlign,
};

pub struct Ui {
    shared: NonNull<UiShared>,
    time: Duration,
    input: Input,
    screen: LogicalRect,
    clip: PhysicalRect,
    scale_factor: f32,
    dirty: DirtyRegions,
    current_id: WidgetId,
    animation_stack: [AnimationCapture; 8],
    animation_depth: usize,
}

impl Ui {
    pub fn platform(&mut self) -> &mut Platform {
        &mut self.shared_mut().platform
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

    /// animates a value toward `target`, keyed by `id`
    ///
    /// the first call starts at `target`. changing the target on a later frame
    /// starts a transition from the current value. render affected content
    /// through the returned scope so its damage bounds can be tracked
    pub fn animate(
        &mut self,
        id: WidgetId,
        target: f32,
        duration: Duration,
        easing: Easing,
    ) -> AnimationScope<'_> {
        let time = self.time;
        begin_animation(self, id, target, |animation| {
            animation.advance(target, duration, easing, time)
        })
    }

    /// animates a repeating value from `0.0` up to `1.0`, keyed by `id`
    ///
    /// render affected content through the returned scope so its damage bounds
    /// can be tracked; `duration` must not be zero
    pub fn animate_loop(
        &mut self,
        id: WidgetId,
        duration: Duration,
        easing: Easing,
    ) -> AnimationScope<'_> {
        let time = self.time;
        begin_animation(self, id, 0.0, |animation| {
            animation.advance_loop(duration, easing, time)
        })
    }

    /// returns `true` once when `duration` has elapsed for `id`
    ///
    /// the timer starts on its first call and is removed when it is not called
    /// during a frame. [`Runtime::next_timer_deadline`] reports when the next
    /// timer needs a frame
    pub fn timer(&mut self, id: WidgetId, duration: Duration) -> bool {
        begin_timer(self, id, duration, None)
    }

    /// returns `true` whenever another `duration` has elapsed for `id`
    ///
    /// missed intervals are coalesced into one event and the next deadline is
    /// scheduled from the frame that observes it. the timer is removed when it
    /// is not called during a frame; `duration` must not be zero
    pub fn timer_loop(&mut self, id: WidgetId, duration: Duration) -> bool {
        assert!(
            !duration.is_zero(),
            "looping timer duration must not be zero"
        );
        begin_timer(self, id, duration, Some(duration))
    }

    /// creates a [`WidgetId`] beneath the current id scope
    pub fn id(&self, source: impl std::hash::Hash) -> WidgetId {
        self.current_id.child(source)
    }

    /// begins a nested id scope derived from `source`
    ///
    /// ids created through the scoped [`Ui`] are children of this scope. the
    /// previous scope is restored when the returned value is dropped
    pub fn begin_scope(&mut self, source: impl std::hash::Hash) -> IdScope<'_> {
        let previous = self.current_id;
        self.current_id = self.current_id.child(source);
        IdScope { ui: self, previous }
    }

    /// limits drawing, interaction, and invalidation to `area`
    ///
    /// the area is intersected with the current clip, allowing scopes to nest
    /// the previous clip is restored when the returned value is dropped
    pub fn begin_clip(&mut self, area: LogicalRect) -> ClipScope<'_> {
        let previous = self.clip;
        self.clip = area
            .to_physical(self.scale_factor)
            .intersection(previous)
            .unwrap_or_default();
        ClipScope {
            ui: self,
            previous,
            rounded: false,
        }
    }

    /// limits drawing to the rounded rectangle and limits interaction and
    /// invalidation to its rectangular bounds
    ///
    /// the bounds are intersected with the current clip, allowing scopes to
    /// nest. the previous clip is restored when the returned value is dropped
    pub fn begin_rounded_clip(
        &mut self,
        area: LogicalRect,
        radius: widgets::BorderRadius,
    ) -> ClipScope<'_> {
        let mut scope = self.begin_clip(area);
        scope.rounded = scope.clip.width > 0 && scope.clip.height > 0;
        if scope.rounded {
            scope.platform().push_rounded_clip(area, radius);
        }
        scope
    }

    pub fn interact(&mut self, id: WidgetId, area: LogicalRect, sense: Sense) -> Interaction {
        let area = area.to_physical(self.scale_factor).intersection(self.clip);
        self.shared_mut().interaction.interact(id, area, sense)
    }

    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.shared().interaction.is_focused(id)
    }

    pub fn focus(&mut self, id: WidgetId) {
        for area in self
            .shared_mut()
            .interaction
            .focus(id)
            .into_iter()
            .flatten()
        {
            self.shared_mut().pending.add(area);
        }
    }

    pub fn clear_focus(&mut self) {
        if let Some(area) = self.shared_mut().interaction.clear_focus() {
            self.shared_mut().pending.add(area);
        }
    }

    pub fn pointer_position(&self) -> Option<LogicalPoint> {
        self.shared().interaction.pointer_position()
    }

    pub fn invalidate(&mut self, area: LogicalRect) {
        if let Some(area) = area.to_physical(self.scale_factor).intersection(self.clip) {
            self.shared_mut().pending.add(area)
        }
    }

    pub fn invalidate_all(&mut self) {
        let clip = self.clip;
        self.shared_mut().pending.add(clip)
    }

    fn record_draw(&mut self, area: LogicalRect) {
        let Some(area) = area.to_physical(self.scale_factor).intersection(self.clip) else {
            return;
        };
        for capture in &mut self.animation_stack[..self.animation_depth] {
            capture.bounds = Some(capture.bounds.map_or(area, |bounds| bounds.union(area)));
            if capture.damage {
                self.dirty.add(area);
            }
        }
    }

    fn shared(&self) -> &UiShared {
        // only used in context of render
        unsafe { self.shared.as_ref() }
    }

    fn shared_mut(&mut self) -> &mut UiShared {
        // only used in context of render
        unsafe { self.shared.as_mut() }
    }

    fn draw_bounds(&self, area: LogicalRect) -> Option<PhysicalRect> {
        let area = area
            .to_physical(self.scale_factor)
            .intersection(self.clip)?;
        self.dirty
            .regions()
            .iter()
            .any(|dirty| area.intersection(*dirty).is_some())
            .then_some(area)
    }
}

#[derive(Clone, Copy, Default)]
struct AnimationCapture {
    index: usize,
    bounds: Option<PhysicalRect>,
    damage: bool,
    changed: bool,
}

pub struct AnimationScope<'a> {
    ui: &'a mut Ui,
    index: usize,
}

impl AnimationScope<'_> {
    pub fn value(&self) -> f32 {
        self.ui.shared().animations[self.index].value
    }

    pub fn is_active(&self) -> bool {
        self.ui.shared().animations[self.index].is_active()
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
        self.ui.animation_depth -= 1;
        let capture = self.ui.animation_stack[self.ui.animation_depth];
        assert_eq!(
            capture.index, self.index,
            "animation scopes must be dropped in reverse order"
        );
        let shared = self.ui.shared_mut();
        let animation = &mut shared.animations[self.index];
        let active = animation.is_active();
        if capture.changed && !active {
            if let Some(area) = animation.previous_bounds {
                shared.pending.add(area);
            }
        }
        if active || capture.changed {
            if let Some(area) = capture.bounds {
                shared.pending.add(area);
            }
        }
        animation.previous_bounds = capture.bounds;
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

pub struct ClipScope<'a> {
    ui: &'a mut Ui,
    previous: PhysicalRect,
    rounded: bool,
}

impl ClipScope<'_> {
    pub fn finish(self) {
        drop(self)
    }
}

impl Deref for ClipScope<'_> {
    type Target = Ui;

    fn deref(&self) -> &Self::Target {
        self.ui
    }
}

impl DerefMut for ClipScope<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ui
    }
}

impl Drop for ClipScope<'_> {
    fn drop(&mut self) {
        if self.rounded {
            self.ui.platform().pop_rounded_clip();
        }
        self.ui.clip = self.previous;
    }
}

fn begin_animation(
    ui: &mut Ui,
    id: WidgetId,
    initial: f32,
    advance: impl FnOnce(&mut animation::AnimationState) -> (bool, bool),
) -> AnimationScope<'_> {
    assert!(
        ui.animation_depth < ui.animation_stack.len(),
        "animation scopes nested too deeply"
    );
    let animations = &mut ui.shared_mut().animations;
    let old_len = animations.len();
    let index = animations
        .iter()
        .position(|animation| animation.id == id)
        .unwrap_or_else(|| {
            animations.push(animation::AnimationState::new(id, initial));
            animations.len() - 1
        });
    assert!(
        !animations[index].seen,
        "duplicate animation WidgetId {id:?}"
    );
    let (damage, changed) = advance(&mut animations[index]);
    let damage = damage || index == old_len;
    if damage {
        if let Some(area) = animations[index]
            .previous_bounds
            .and_then(|area| area.intersection(ui.clip))
        {
            ui.dirty.add(area);
        }
    }
    ui.animation_stack[ui.animation_depth] = AnimationCapture {
        index,
        bounds: None,
        damage,
        changed,
    };
    ui.animation_depth += 1;
    AnimationScope { ui, index }
}

fn begin_timer(ui: &mut Ui, id: WidgetId, duration: Duration, interval: Option<Duration>) -> bool {
    let time = ui.time;
    let timers = &mut ui.shared_mut().timers;
    let timer = if let Some(timer) = timers.iter_mut().find(|timer| timer.id == id) {
        timer
    } else {
        timers.push(timer::TimerState::new(id, duration, interval, time));
        timers.last_mut().unwrap()
    };
    assert!(!timer.seen, "duplicate timer WidgetId {id:?}");
    timer.advance(duration, interval, time)
}

//
// runtime
//

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepaintBuffer {
    #[default]
    Reused,
    Swapped,
}

pub struct Runtime {
    shared: UiShared,
    repaint_buffer: RepaintBuffer,
    screen: LogicalRect,
    physical_screen: PhysicalRect,
    scale_factor: f32,
    previous: DirtyRegions,
}

struct UiShared {
    platform: Platform,
    interaction: interaction::InteractionState,
    animations: Vec<animation::AnimationState>,
    timers: Vec<timer::TimerState>,
    pending: DirtyRegions,
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
            shared: UiShared {
                platform,
                interaction: interaction::InteractionState::default(),
                animations: Vec::new(),
                timers: Vec::new(),
                pending,
            },
            repaint_buffer: RepaintBuffer::default(),
            screen,
            physical_screen,
            scale_factor,
            previous: DirtyRegions::default(),
        }
    }

    pub fn with_repaint_buffer(mut self, repaint_buffer: RepaintBuffer) -> Self {
        self.repaint_buffer = repaint_buffer;
        self
    }

    pub fn platform(&mut self) -> &mut Platform {
        &mut self.shared.platform
    }

    pub fn render<R>(
        &mut self,
        time: Duration,
        input: Input,
        render: impl FnOnce(&mut Ui) -> R,
    ) -> R {
        let pending = std::mem::take(&mut self.shared.pending);
        let mut dirty = if self.repaint_buffer == RepaintBuffer::Swapped {
            std::mem::take(&mut self.previous)
        } else {
            DirtyRegions::default()
        };
        dirty.extend(&pending);
        if self.repaint_buffer == RepaintBuffer::Swapped {
            self.previous = pending;
        }
        for animation in &mut self.shared.animations {
            animation.seen = false;
        }
        for timer in &mut self.shared.timers {
            timer.seen = false;
        }
        let interaction_damage = self
            .shared
            .interaction
            .begin_frame(&input, self.scale_factor);
        for area in interaction_damage.into_iter().flatten() {
            dirty.add(area);
            if self.repaint_buffer == RepaintBuffer::Swapped {
                self.previous.add(area);
            }
        }
        self.shared.platform.begin_frame(dirty.regions());
        let mut ui = Ui {
            shared: NonNull::from(&mut self.shared),
            time,
            input,
            screen: self.screen,
            clip: self.physical_screen,
            scale_factor: self.scale_factor,
            dirty,
            current_id: WidgetId::new("blit root"),
            animation_stack: [AnimationCapture::default(); 8],
            animation_depth: 0,
        };
        let output = render(&mut ui);
        assert_eq!(ui.animation_depth, 0, "animation scope was not dropped");
        let shared = ui.shared_mut();
        for area in shared
            .interaction
            .end_frame(self.scale_factor)
            .into_iter()
            .flatten()
        {
            shared.pending.add(area);
        }
        let pending = &mut shared.pending;
        shared.animations.retain(|animation| {
            if !animation.seen {
                if let Some(area) = animation.previous_bounds {
                    pending.add(area);
                }
            }
            animation.seen
        });
        shared.timers.retain(|timer| timer.seen);
        self.shared.platform.end_frame();
        output
    }

    pub fn render_batch(
        &mut self,
        time: Duration,
        inputs: impl IntoIterator<Item = Input>,
        mut render: impl FnMut(&mut Ui),
    ) {
        let mut inputs = inputs.into_iter();
        let Some(input) = inputs.next() else { return };

        self.render(time, input, &mut render);
        let mut previous = std::mem::take(&mut self.previous);
        for input in inputs {
            self.render(time, input, &mut render);
            previous.extend(&self.previous);
            self.previous = DirtyRegions::default();
        }
        self.previous = previous;
    }

    pub fn has_pending_redraw(&self) -> bool {
        !self.shared.pending.is_empty()
            || self.repaint_buffer == RepaintBuffer::Swapped && !self.previous.is_empty()
            || self
                .shared
                .animations
                .iter()
                .any(animation::AnimationState::is_active)
    }

    pub fn next_timer_deadline(&self) -> Option<Duration> {
        self.shared
            .timers
            .iter()
            .filter_map(timer::TimerState::deadline)
            .min()
    }

    pub fn invalidate(&mut self, area: LogicalRect) {
        if let Some(area) = area
            .to_physical(self.scale_factor)
            .intersection(self.physical_screen)
        {
            self.shared.pending.add(area)
        }
    }

    pub fn invalidate_all(&mut self) {
        self.shared.pending.add(self.physical_screen)
    }

    pub fn screen(&self) -> LogicalRect {
        self.screen
    }
}
