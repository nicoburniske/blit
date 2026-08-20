pub mod animation;
pub mod color;
pub mod command_list;
mod container;
pub mod geometry;
pub(crate) mod graph;
pub mod input;
pub mod interact;
pub mod keyboard;
pub mod paint;
pub mod platform;
pub mod resource;
mod style;
#[cfg(test)]
mod test;
mod timer;
pub mod widget;

pub use container::{Align, Axis, Container, Item, Justify, Scope, Sizing};
#[doc(hidden)]
pub use graph::{Content, ImageContent, NodeId, TextCaret, TextContent, TextSelection};
pub use style::{Appearance, Clip, Shadow};

use std::{ptr::NonNull, time::Duration};

use animation::Easing;
use command_list::{CommandDiffConfig, CommandList, CommandListDiffer};
use geometry::{LogicalPoint, LogicalRect, PhysicalRect};
use input::Input;
use interact::{Interaction, Sense, WidgetId};
use platform::{Platform, PlatformImpl};

pub struct Ui {
    shared: NonNull<UiShared>,
    time: Duration,
    input: Input,
    screen: LogicalRect,
}

impl Ui {
    pub fn add<W: widget::Widget>(&mut self, widget: W) -> W::Output {
        widget.build(self)
    }

    /// restores damaged screen pixels to the render target's default value before painting
    pub fn clear(&mut self) {
        self.frame_mut().clear()
    }

    pub fn row(&mut self, container: Container<'_>) -> Scope<'_> {
        container::open(self, Axis::Horizontal, container)
    }

    pub fn column(&mut self, container: Container<'_>) -> Scope<'_> {
        container::open(self, Axis::Vertical, container)
    }

    pub fn flow(&mut self, axis: Axis, container: Container<'_>) -> Scope<'_> {
        container::open(self, axis, container)
    }

    pub fn geometry(&self, id: WidgetId) -> Option<LogicalRect> {
        self.shared().geometry.get(id)
    }

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
    pub fn animate(
        &mut self,
        id: WidgetId,
        target: f32,
        duration: Duration,
        easing: Easing,
    ) -> f32 {
        let time = self.time;
        begin_animation(self, id, target, |animation| {
            animation.advance(target, duration, easing, time)
        })
    }

    /// animates a repeating value from `0.0` up to `1.0`, keyed by `id`
    ///
    /// zero duration stops the loop and resets the value
    pub fn animate_loop(&mut self, id: WidgetId, duration: Duration, easing: Easing) -> f32 {
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

    pub fn is_focused(&self, id: WidgetId) -> bool {
        self.shared().interaction.is_focused(id)
    }

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

    pub fn pointer_position(&self) -> Option<LogicalPoint> {
        self.shared().interaction.pointer_position()
    }

    pub fn request_frame(&mut self) {
        self.shared_mut().frame_requested = true
    }

    pub fn invalidate_all(&mut self) {
        let shared = self.shared_mut();
        shared.frame_requested = true;
        shared.full_repaint = true;
    }
}

#[doc(hidden)]
impl Ui {
    pub fn add_leaf(&mut self, item: Item, content: Content) -> NodeId {
        self.frame_mut().add_leaf(item, content)
    }

    pub fn open_container(
        &mut self,
        axis: Axis,
        container: Container<'_>,
    ) -> (NodeId, Interaction) {
        let interaction = container
            .interaction
            .map_or_default(|(id, sense)| self.shared_mut().interaction.response(id, sense));
        let node = self.frame_mut().add_container(axis, container);
        (node, interaction)
    }

    pub fn close_container(&mut self, node: NodeId) {
        self.frame_mut().close(node)
    }

    pub fn set_node_id(&mut self, node: NodeId, id: WidgetId) {
        self.frame_mut().set_id(node, id)
    }

    pub fn interact_node(&mut self, node: NodeId, id: WidgetId, sense: Sense) -> Interaction {
        let interaction = self.shared_mut().interaction.response(id, sense);
        self.frame_mut().set_interaction(node, id, sense);
        interaction
    }

    pub fn set_node_appearance(&mut self, node: NodeId, appearance: Appearance<'_>) {
        self.frame_mut().set_appearance(node, appearance)
    }

    fn shared(&self) -> &UiShared {
        // only used in context of render
        unsafe { self.shared.as_ref() }
    }

    fn shared_mut(&mut self) -> &mut UiShared {
        // only used in context of render
        unsafe { self.shared.as_mut() }
    }

    fn frame_mut(&mut self) -> &mut graph::FrameGraph {
        &mut self.shared_mut().frame
    }
}

fn begin_animation(
    ui: &mut Ui,
    id: WidgetId,
    initial: f32,
    advance: impl FnOnce(&mut animation::AnimationState),
) -> f32 {
    let animations = &mut ui.shared_mut().animations;
    let index = match animations.binary_search_by_key(&id, |animation| animation.id) {
        Ok(index) => index,
        Err(index) => {
            animations.insert(index, animation::AnimationState::new(id, initial));
            index
        }
    };
    assert!(
        !animations[index].seen,
        "duplicate animation WidgetId {id:?}"
    );
    advance(&mut animations[index]);
    animations[index].value
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepaintBuffer {
    /// the same buffer retains the previously rendered frame
    #[default]
    Reused,
    /// two buffers alternate, so each frame also repairs the previous frame's damage
    Swapped,
}

/// retained frame graph memory after its buffers have grown
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameGraphMemory {
    pub node_size: usize,
    pub node_capacity: usize,
    pub heap_bytes: usize,
}

pub struct Runtime<P: PlatformImpl> {
    platform: Box<P>,
    shared: UiShared,
    repaint_buffer: RepaintBuffer,
    screen: LogicalRect,
    physical_screen: PhysicalRect,
    scale_factor: f32,
    previous_commands: CommandList,
    differ: CommandListDiffer,
    previous_damage: Vec<PhysicalRect>,
    render_damage: Vec<PhysicalRect>,
}

struct UiShared {
    platform: Platform,
    frame: graph::FrameGraph,
    commands: CommandList,
    interaction: interact::InteractionState,
    geometry: graph::GeometryState,
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
                frame: graph::FrameGraph::default(),
                commands: CommandList::default(),
                interaction: interact::InteractionState::default(),
                geometry: graph::GeometryState::default(),
                animations: Vec::new(),
                timers: Vec::new(),
                frame_requested: true,
                full_repaint: true,
            },
            repaint_buffer,
            screen,
            physical_screen,
            scale_factor,
            previous_commands: CommandList::default(),
            differ: CommandListDiffer::default(),
            previous_damage: Vec::new(),
            render_damage: Vec::new(),
        }
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

    pub fn request_frame(&mut self) {
        self.shared.frame_requested = true
    }

    pub fn invalidate_all(&mut self) {
        self.shared.frame_requested = true;
        self.shared.full_repaint = true;
    }

    pub fn set_command_diff_config(&mut self, config: CommandDiffConfig) {
        self.differ.set_config(config);
    }

    pub fn refresh_screen(&mut self) {
        let physical_screen = self.platform.screen();
        let scale_factor = self.platform.scale_factor();
        assert!(scale_factor.is_finite() && scale_factor > 0.0);
        if self.physical_screen == physical_screen && self.scale_factor == scale_factor {
            return;
        }
        self.physical_screen = physical_screen;
        self.screen = physical_screen.to_logical(scale_factor);
        self.scale_factor = scale_factor;
        self.previous_damage.clear();
        self.invalidate_all();
    }

    pub fn screen(&self) -> LogicalRect {
        self.screen
    }

    pub fn frame_graph_memory(&self) -> FrameGraphMemory {
        self.shared.frame.memory()
    }

    fn record<R>(&mut self, time: Duration, input: Input, render: impl FnOnce(&mut Ui) -> R) -> R {
        self.shared.commands.clear();
        self.shared.frame.begin(self.screen);
        for animation in &mut self.shared.animations {
            animation.seen = false;
        }
        for timer in &mut self.shared.timers {
            timer.seen = false;
        }
        self.shared
            .interaction
            .begin_frame(&input, self.scale_factor);
        let mut ui = Ui {
            shared: NonNull::from(&mut self.shared),
            time,
            input,
            screen: self.screen,
        };
        let output = render(&mut ui);
        {
            let shared = ui.shared_mut();
            let mut frame = std::mem::take(&mut shared.frame);
            frame.finish(
                &mut shared.platform,
                &mut shared.commands,
                &mut shared.interaction,
                &mut shared.geometry,
                self.scale_factor,
            );
            shared.frame = frame;
            if shared.interaction.end_frame(self.scale_factor) {
                shared.frame_requested = true;
            }
            shared.geometry.end_frame();
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
            self.render_damage.extend_from_slice(
                self.differ
                    .diff(&self.previous_commands, &self.shared.commands),
            );
        }
        let current_damage_len = self.render_damage.len();
        if self.repaint_buffer == RepaintBuffer::Swapped {
            self.render_damage.extend_from_slice(&self.previous_damage);
        }
        self.platform
            .render(&self.shared.commands, &self.render_damage);
        self.previous_damage.clear();
        if self.repaint_buffer == RepaintBuffer::Swapped {
            self.previous_damage
                .extend_from_slice(&self.render_damage[..current_damage_len]);
        }
        std::mem::swap(&mut self.previous_commands, &mut self.shared.commands);
    }
}
