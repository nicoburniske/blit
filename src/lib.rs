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

pub use animation::{Easing, Transition};
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
    /// the first call snaps to `target`. later changes animate from the current
    /// value when `duration` is non-zero and snap when it is zero
    ///
    /// rendering through the returned scope tracks damage when the value changes
    /// and while it is animating. adding or removing the content must be
    /// invalidated by the caller
    pub fn animate(
        &mut self,
        id: WidgetId,
        target: f32,
        duration: Duration,
        easing: Easing,
    ) -> AnimationScope<'_> {
        let time = self.time;
        begin_animations(self, [id], [target], |_, animation| {
            animation.advance(target, duration, easing, time)
        })
    }

    /// animates independent values through one damage scope, keyed by `id`
    ///
    /// each value follows [`Ui::animate`] with its own target, duration, and
    /// easing. array order must remain stable between frames
    pub fn animate_values<const N: usize>(
        &mut self,
        id: WidgetId,
        transitions: [Transition; N],
    ) -> AnimationScope<'_, N> {
        let time = self.time;
        let ids = std::array::from_fn(|index| id.child(("animation component", index)));
        let initial = transitions.map(|transition| transition.target);
        begin_animations(self, ids, initial, |index, animation| {
            let transition = transitions[index];
            animation.advance(
                transition.target,
                transition.duration,
                transition.easing,
                time,
            )
        })
    }

    /// animates a repeating value from `0.0` up to `1.0`, keyed by `id`
    ///
    /// render affected content through the returned scope so its damage bounds
    /// can be tracked
    ///
    /// zero duration stops the loop and resets the value
    pub fn animate_loop(
        &mut self,
        id: WidgetId,
        duration: Duration,
        easing: Easing,
    ) -> AnimationScope<'_> {
        let time = self.time;
        begin_animations(self, [id], [0.0], |_, animation| {
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

    /// records `area` as drawn and returns its clipped physical bounds when it
    /// intersects this frame's damage
    pub fn draw_bounds(&mut self, area: LogicalRect) -> Option<PhysicalRect> {
        let area = area
            .to_physical(self.scale_factor)
            .intersection(self.clip)?;
        let mut damaged = false;
        for capture in &mut self.animation_stack[..self.animation_depth] {
            capture.bounds = Some(capture.bounds.map_or(area, |bounds| bounds.union(area)));
            if capture.damage {
                damaged = true;
                self.dirty.add(area);
            }
        }
        if damaged {
            self.platform().add_damage(area);
        }
        self.dirty
            .regions()
            .iter()
            .any(|dirty| area.intersection(*dirty).is_some())
            .then_some(area)
    }
}

#[derive(Clone, Copy, Default)]
struct AnimationCapture {
    bounds: Option<PhysicalRect>,
    damage: bool,
    changed: bool,
}

pub struct AnimationScope<'a, const N: usize = 1> {
    ui: &'a mut Ui,
    ids: [WidgetId; N],
    values: [f32; N],
    active: [bool; N],
    depth: usize,
}

impl AnimationScope<'_, 1> {
    pub fn value(&self) -> f32 {
        self.values[0]
    }
}

impl<const N: usize> AnimationScope<'_, N> {
    pub fn values(&self) -> [f32; N] {
        self.values
    }

    pub fn is_active(&self) -> bool {
        self.active.iter().any(|active| *active)
    }

    pub fn finish(self) {
        drop(self)
    }
}

impl<const N: usize> Deref for AnimationScope<'_, N> {
    type Target = Ui;

    fn deref(&self) -> &Self::Target {
        self.ui
    }
}

impl<const N: usize> DerefMut for AnimationScope<'_, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ui
    }
}

impl<const N: usize> Drop for AnimationScope<'_, N> {
    fn drop(&mut self) {
        assert_eq!(
            self.ui.animation_depth,
            self.depth + 1,
            "animation scopes must be dropped in reverse order"
        );
        self.ui.animation_depth = self.depth;
        let capture = self.ui.animation_stack[self.depth];
        let shared = self.ui.shared_mut();
        for id in &self.ids {
            let index = shared
                .animations
                .binary_search_by_key(id, |animation| animation.id)
                .expect("animation state disappeared while its scope was active");
            shared.animations[index].previous_bounds = capture.bounds;
        }
        if (capture.damage || capture.changed)
            && let Some(area) = capture.bounds
        {
            shared.pending.add(area);
        }
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

fn begin_animations<const N: usize>(
    ui: &mut Ui,
    ids: [WidgetId; N],
    initial: [f32; N],
    mut advance: impl FnMut(usize, &mut animation::AnimationState),
) -> AnimationScope<'_, N> {
    assert!(N != 0, "animation groups must contain at least one value");
    assert!(
        ui.animation_depth < ui.animation_stack.len(),
        "animation scopes nested too deeply"
    );

    let mut values = [0.0; N];
    let mut active = [false; N];
    let mut damage = false;
    let mut changed = false;
    for component in 0..N {
        let id = ids[component];
        let (value, component_active, component_damage, value_changed, previous_damage) = {
            let animations = &mut ui.shared_mut().animations;
            let index = match animations.binary_search_by_key(&id, |animation| animation.id) {
                Ok(index) => index,
                Err(index) => {
                    animations.insert(
                        index,
                        animation::AnimationState::new(id, initial[component]),
                    );
                    index
                }
            };
            assert!(
                !animations[index].seen,
                "duplicate animation WidgetId {id:?}"
            );
            let was_active = animations[index].is_active();
            let previous_value = animations[index].value;
            advance(component, &mut animations[index]);
            let component_active = animations[index].is_active();
            let value_changed = animations[index].value != previous_value;
            (
                animations[index].value,
                component_active,
                was_active || component_active,
                value_changed,
                if !was_active && value_changed {
                    animations[index].previous_bounds
                } else {
                    None
                },
            )
        };
        values[component] = value;
        active[component] = component_active;
        damage |= component_damage;
        changed |= value_changed;
        if let Some(area) = previous_damage.and_then(|area| area.intersection(ui.clip)) {
            ui.shared_mut().pending.add(area);
        }
    }

    let depth = ui.animation_depth;
    ui.animation_stack[depth] = AnimationCapture {
        bounds: None,
        damage,
        changed,
    };
    ui.animation_depth += 1;
    AnimationScope {
        ui,
        ids,
        values,
        active,
        depth,
    }
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
// internals
//

impl Ui {
    fn shared(&self) -> &UiShared {
        // only used in context of render
        unsafe { self.shared.as_ref() }
    }

    fn shared_mut(&mut self) -> &mut UiShared {
        // only used in context of render
        unsafe { self.shared.as_mut() }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepaintBuffer {
    #[default]
    Reused,
    Swapped,
}

pub struct Runtime<P: PlatformImpl> {
    platform: Box<P>,
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

impl<P: PlatformImpl + 'static> Runtime<P> {
    pub fn new(platform: P) -> Self {
        let mut platform = Box::new(platform);
        let physical_screen = platform.screen();
        let scale_factor = platform.scale_factor();
        assert!(scale_factor.is_finite() && scale_factor > 0.0);
        let screen = physical_screen.to_logical(scale_factor);
        let mut pending = DirtyRegions::default();
        pending.add(physical_screen);
        let erased_platform = Platform::new(platform.as_mut());
        Self {
            platform,
            shared: UiShared {
                platform: erased_platform,
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

    pub fn platform(&mut self) -> &mut P {
        self.platform.as_mut()
    }

    pub fn erased_platform(&mut self) -> &mut Platform {
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
            if animation.is_active()
                && let Some(area) = animation.previous_bounds
            {
                dirty.add(area);
            }
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
            if !animation.seen
                && animation.is_active()
                && let Some(area) = animation.previous_bounds
            {
                pending.add(area);
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
