pub mod animation;
pub mod color;
pub mod geometry;
pub mod input;
pub mod interact;
pub mod keyboard;
pub mod layout;
pub mod paint;
pub mod paint_list;
pub mod platform;
pub mod resource;
#[cfg(test)]
mod test;
mod timer;
pub mod widget;

use std::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
    time::Duration,
};

use animation::{Easing, Transition};
use geometry::{LogicalPoint, LogicalRect, PhysicalRect};
use input::Input;
use interact::{Interaction, Sense, WidgetId};
use paint_list::{ClipId, PaintList, PaintListDiffer};
use platform::{Platform, PlatformImpl};

pub struct Ui {
    shared: NonNull<UiShared>,
    time: Duration,
    input: Input,
    screen: LogicalRect,
    clip: PhysicalRect,
    scale_factor: f32,
    paint_clip: ClipId,
    current_id: WidgetId,
}

impl Ui {
    pub fn platform(&mut self) -> &mut Platform { &mut self.shared_mut().platform }

    pub fn screen(&self) -> LogicalRect { self.screen }

    pub fn input(&self) -> &Input { &self.input }

    pub fn time(&self) -> Duration { self.time }

    /// animates a value toward `target`, keyed by `id`
    ///
    /// the first call snaps to `target`. later changes animate from the current
    /// value when `duration` is non-zero and snap when it is zero
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

    /// animates independent values keyed by `id`
    ///
    /// each value follows [`Ui::animate`] with its own target, duration, and
    /// easing. array order must remain stable between frames
    pub fn animate_values<const N: usize>(
        &mut self,
        id: WidgetId,
        transitions: [Transition; N],
    ) -> AnimationScope<'_, N> {
        let time = self.time;
        let ids = std::array::from_fn(|index| id.child(("animation value", index)));
        let initial = transitions.map(|transition| transition.target);
        begin_animations(self, ids, initial, |index, animation| {
            let transition = transitions[index];
            animation.advance(transition.target, transition.duration, transition.easing, time)
        })
    }

    /// animates a repeating value from `0.0` up to `1.0`, keyed by `id`
    ///
    /// zero duration stops the loop and resets the value
    pub fn animate_loop(&mut self, id: WidgetId, duration: Duration, easing: Easing) -> AnimationScope<'_> {
        let time = self.time;
        begin_animations(self, [id], [0.0], |_, animation| animation.advance_loop(duration, easing, time))
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
        assert!(!duration.is_zero(), "looping timer duration must not be zero");
        begin_timer(self, id, duration, Some(duration))
    }

    /// creates a [`WidgetId`] beneath the current id scope
    pub fn id(&self, source: impl std::hash::Hash) -> WidgetId { self.current_id.child(source) }

    /// begins a nested id scope derived from `source`
    ///
    /// ids created through the scoped [`Ui`] are children of this scope. the
    /// previous scope is restored when the returned value is dropped
    pub fn begin_scope(&mut self, source: impl std::hash::Hash) -> IdScope<'_> {
        let previous = self.current_id;
        self.current_id = self.current_id.child(source);
        IdScope { ui: self, previous }
    }

    /// limits drawing and interaction to `area`
    ///
    /// the area is intersected with the current clip, allowing scopes to nest
    /// the previous clip is restored when the returned value is dropped
    pub fn begin_clip(&mut self, area: LogicalRect) -> ClipScope<'_> {
        let previous = self.clip;
        let previous_paint_clip = self.paint_clip;
        self.clip = area.to_physical(self.scale_factor).intersection(previous).unwrap_or_default();
        ClipScope { ui: self, previous, previous_paint_clip }
    }

    /// limits drawing to the rounded rectangle and interaction to its bounds
    ///
    /// the bounds are intersected with the current clip, allowing scopes to
    /// nest. the previous clip is restored when the returned value is dropped
    pub fn begin_rounded_clip(&mut self, area: LogicalRect, radius: paint::BorderRadius) -> ClipScope<'_> {
        let mut scope = self.begin_clip(area);
        if scope.clip.width > 0 && scope.clip.height > 0 {
            let parent = scope.paint_clip;
            scope.paint_clip = scope.paint_mut().push_clip(parent, area, radius);
        }
        scope
    }

    pub fn interact(&mut self, id: WidgetId, area: LogicalRect, sense: Sense) -> Interaction {
        let area = area.to_physical(self.scale_factor).intersection(self.clip);
        self.shared_mut().interaction.interact(id, area, sense)
    }

    pub fn is_focused(&self, id: WidgetId) -> bool { self.shared().interaction.is_focused(id) }

    pub fn focus(&mut self, id: WidgetId) {
        if self.shared_mut().interaction.focus(id) {
            self.request_frame();
        }
    }

    pub fn clear_focus(&mut self) {
        if self.shared_mut().interaction.clear_focus() {
            self.request_frame();
        }
    }

    pub fn pointer_position(&self) -> Option<LogicalPoint> { self.shared().interaction.pointer_position() }

    pub fn request_frame(&mut self) { self.shared_mut().frame_requested = true }

    pub fn invalidate_all(&mut self) {
        let shared = self.shared_mut();
        shared.frame_requested = true;
        shared.full_repaint = true;
    }

    pub fn paint_rectangle(&mut self, rectangle: paint::Rectangle<'_>) {
        let Some(bounds) = rectangle.area.to_physical(self.scale_factor).intersection(self.clip) else {
            return;
        };
        let clip = self.paint_clip;
        self.paint_mut().push_rectangle(rectangle, bounds, clip);
    }

    pub fn paint_box_shadow(&mut self, shadow: paint::BoxShadow) {
        let Some(bounds) = shadow.bounds().to_physical(self.scale_factor).intersection(self.clip) else {
            return;
        };
        let clip = self.paint_clip;
        self.paint_mut().push_box_shadow(shadow, bounds, clip);
    }

    pub fn paint_image(&mut self, image: paint::ImageRequest) {
        let Some(bounds) = image.area.to_physical(self.scale_factor).intersection(self.clip) else {
            return;
        };
        let clip = self.paint_clip;
        self.paint_mut().push_image(image, bounds, clip);
    }

    pub fn paint_text(&mut self, text: paint::TextRequest) {
        let Some(bounds) = text.area.to_physical(self.scale_factor).intersection(self.clip) else {
            return;
        };
        let clip = self.paint_clip;
        self.paint_mut().push_text(text, bounds, clip);
    }
}

pub struct AnimationScope<'a, const N: usize = 1> {
    ui: &'a mut Ui,
    values: [f32; N],
    active: [bool; N],
}

impl AnimationScope<'_, 1> {
    pub fn value(&self) -> f32 { self.values[0] }
}

impl<const N: usize> AnimationScope<'_, N> {
    pub fn values(&self) -> [f32; N] { self.values }

    pub fn is_active(&self) -> bool { self.active.iter().any(|active| *active) }

    pub fn finish(self) {}
}

impl<const N: usize> Deref for AnimationScope<'_, N> {
    type Target = Ui;

    fn deref(&self) -> &Self::Target { self.ui }
}

impl<const N: usize> DerefMut for AnimationScope<'_, N> {
    fn deref_mut(&mut self) -> &mut Self::Target { self.ui }
}

pub struct IdScope<'a> {
    ui: &'a mut Ui,
    previous: WidgetId,
}

impl IdScope<'_> {
    pub fn ui(&mut self) -> &mut Ui { self.ui }

    pub fn finish(self) { drop(self) }
}

impl Drop for IdScope<'_> {
    fn drop(&mut self) { self.ui.current_id = self.previous; }
}

pub struct ClipScope<'a> {
    ui: &'a mut Ui,
    previous: PhysicalRect,
    previous_paint_clip: ClipId,
}

impl ClipScope<'_> {
    pub fn finish(self) { drop(self) }
}

impl Deref for ClipScope<'_> {
    type Target = Ui;

    fn deref(&self) -> &Self::Target { self.ui }
}

impl DerefMut for ClipScope<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target { self.ui }
}

impl Drop for ClipScope<'_> {
    fn drop(&mut self) {
        self.ui.clip = self.previous;
        self.ui.paint_clip = self.previous_paint_clip;
    }
}

fn begin_animations<const N: usize>(
    ui: &mut Ui,
    ids: [WidgetId; N],
    initial: [f32; N],
    mut advance: impl FnMut(usize, &mut animation::AnimationState),
) -> AnimationScope<'_, N> {
    assert!(N != 0, "animation groups must contain at least one value");

    let mut values = [0.0; N];
    let mut active = [false; N];
    for value_index in 0..N {
        let id = ids[value_index];
        let (value, value_active) = {
            let animations = &mut ui.shared_mut().animations;
            let index = match animations.binary_search_by_key(&id, |animation| animation.id) {
                Ok(index) => index,
                Err(index) => {
                    animations.insert(index, animation::AnimationState::new(id, initial[value_index]));
                    index
                }
            };
            assert!(!animations[index].seen, "duplicate animation WidgetId {id:?}");
            advance(value_index, &mut animations[index]);
            (animations[index].value, animations[index].is_active())
        };
        values[value_index] = value;
        active[value_index] = value_active;
    }

    AnimationScope { ui, values, active }
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

    fn paint_mut(&mut self) -> &mut PaintList { &mut self.shared_mut().paint }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepaintBuffer {
    /// the same buffer retains the previously rendered frame
    #[default]
    Reused,
    /// two buffers alternate, so each frame also repairs the previous frame's damage
    Swapped,
}

pub struct Runtime<P: PlatformImpl> {
    platform: Box<P>,
    shared: UiShared,
    repaint_buffer: RepaintBuffer,
    screen: LogicalRect,
    physical_screen: PhysicalRect,
    scale_factor: f32,
    previous_paint: PaintList,
    differ: PaintListDiffer,
    previous_damage: Vec<PhysicalRect>,
    render_damage: Vec<PhysicalRect>,
}

struct UiShared {
    platform: Platform,
    paint: PaintList,
    interaction: interact::InteractionState,
    animations: Vec<animation::AnimationState>,
    timers: Vec<timer::TimerState>,
    frame_requested: bool,
    full_repaint: bool,
}

impl<P: PlatformImpl + 'static> Runtime<P> {
    pub fn new(platform: P) -> Self {
        let repaint_buffer = platform.repaint_buffer();
        let mut platform = Box::new(platform);
        let physical_screen = platform.screen();
        let scale_factor = platform.scale_factor();
        assert!(scale_factor.is_finite() && scale_factor > 0.0);
        let screen = physical_screen.to_logical(scale_factor);
        let erased_platform = Platform::new(platform.as_mut());
        Self {
            platform,
            shared: UiShared {
                platform: erased_platform,
                paint: PaintList::default(),
                interaction: interact::InteractionState::default(),
                animations: Vec::new(),
                timers: Vec::new(),
                frame_requested: true,
                full_repaint: true,
            },
            repaint_buffer,
            screen,
            physical_screen,
            scale_factor,
            previous_paint: PaintList::default(),
            differ: PaintListDiffer::default(),
            previous_damage: Vec::new(),
            render_damage: Vec::new(),
        }
    }

    pub fn platform(&mut self) -> &mut P { self.platform.as_mut() }

    pub fn erased_platform(&mut self) -> &mut Platform { &mut self.shared.platform }

    pub fn render<R>(&mut self, time: Duration, input: Input, render: impl FnOnce(&mut Ui) -> R) -> R {
        self.shared.frame_requested = false;
        let output = self.record(time, input, render);
        self.commit();
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
        self.shared.frame_requested = false;
        self.record(time, input, &mut render);
        for input in inputs {
            self.record(time, input, &mut render);
        }
        self.commit();
    }

    pub fn has_pending_redraw(&self) -> bool {
        self.shared.frame_requested
            || self.shared.full_repaint
            || self.repaint_buffer == RepaintBuffer::Swapped && !self.previous_damage.is_empty()
            || self.shared.animations.iter().any(animation::AnimationState::is_active)
    }

    pub fn next_timer_deadline(&self) -> Option<Duration> {
        self.shared.timers.iter().filter_map(timer::TimerState::deadline).min()
    }

    pub fn request_frame(&mut self) { self.shared.frame_requested = true }

    pub fn invalidate_all(&mut self) {
        self.shared.frame_requested = true;
        self.shared.full_repaint = true;
    }

    pub fn screen(&self) -> LogicalRect { self.screen }

    fn record<R>(&mut self, time: Duration, input: Input, render: impl FnOnce(&mut Ui) -> R) -> R {
        self.shared.paint.clear();
        for animation in &mut self.shared.animations {
            animation.seen = false;
        }
        for timer in &mut self.shared.timers {
            timer.seen = false;
        }
        self.shared.interaction.begin_frame(&input, self.scale_factor);
        let mut ui = Ui {
            shared: NonNull::from(&mut self.shared),
            time,
            input,
            screen: self.screen,
            clip: self.physical_screen,
            scale_factor: self.scale_factor,
            paint_clip: ClipId::default(),
            current_id: WidgetId::new("blit root"),
        };
        let output = render(&mut ui);
        {
            let shared = ui.shared_mut();
            if shared.interaction.end_frame(self.scale_factor) {
                shared.frame_requested = true;
            }
            shared.animations.retain(|animation| animation.seen);
            shared.timers.retain(|timer| timer.seen);
        }
        output
    }

    fn commit(&mut self) {
        self.render_damage.clear();
        if std::mem::take(&mut self.shared.full_repaint) {
            self.render_damage.push(self.physical_screen);
        } else {
            self.render_damage.extend_from_slice(self.differ.diff(&self.previous_paint, &self.shared.paint));
        }
        let current_damage_len = self.render_damage.len();
        if self.repaint_buffer == RepaintBuffer::Swapped {
            self.render_damage.extend_from_slice(&self.previous_damage);
        }
        self.platform.render(&self.shared.paint, &self.render_damage);
        self.previous_damage.clear();
        if self.repaint_buffer == RepaintBuffer::Swapped {
            self.previous_damage.extend_from_slice(&self.render_damage[..current_damage_len]);
        }
        std::mem::swap(&mut self.previous_paint, &mut self.shared.paint);
    }
}
