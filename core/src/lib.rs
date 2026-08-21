pub mod animation;
mod builder;
pub mod color;
pub mod command_list;
mod container;
pub mod geometry;
pub(crate) mod graph;
pub mod input;
pub mod interact;
pub mod paint;
pub mod platform;
pub mod resource;
mod style;
#[cfg(test)]
mod test;
mod timer;
pub mod widget;

pub use container::{
    Absolute, Align, Anchor, Axis, Container, ContainerConfig, Item, Justify, PositionTarget,
    Scope, Sizing,
};
#[doc(hidden)]
pub use graph::{Content, ImageContent, NodeId, TextCaret, TextContent, TextSelection};
pub use style::{Clip, Shadow, Style};

use std::{ptr::NonNull, time::Duration};

use animation::Easing;
use command_list::{CommandDiffConfig, CommandList, CommandListDiffer};
use geometry::{LogicalPoint, LogicalRect, PhysicalRect};
use input::Input;
use interact::{Interaction, Sense, WidgetId};
use paint::{TextRunId, TextStyle};
use platform::PlatformImpl;
use resource::{ImageData, ImageHandle};

pub struct Ui {
    state: NonNull<UiState>,
    platform: NonNull<dyn PlatformImpl>,
    time: Duration,
    input: Input,
}

impl Ui {
    pub fn add<W: widget::Widget>(&mut self, widget: W) -> W::Output {
        widget.build(self)
    }

    /// restores damaged screen pixels to the render target's default value before painting
    pub fn clear(&mut self) {
        self.frame_mut().clear()
    }

    pub fn container(&mut self) -> Container<'_, '_> {
        Container::new(self)
    }

    pub fn geometry(&self, id: WidgetId) -> Option<LogicalRect> {
        self.state().geometry.get(id)
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        self.platform_mut().create_image(data)
    }

    pub fn screen(&self) -> LogicalRect {
        self.state().screen
    }

    pub fn input(&self) -> &Input {
        &self.input
    }

    pub fn interact(&mut self, id: WidgetId, sense: Sense) -> Interaction {
        self.state_mut().interaction.response(id, sense)
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
    /// during a frame. [`UiState::next_timer_deadline`] reports when the next
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
        self.state().interaction.is_focused(id)
    }

    pub fn focus(&mut self, id: WidgetId) {
        if self.state_mut().interaction.focus(id) {
            self.request_frame();
        }
    }

    pub fn clear_focus(&mut self) {
        if self.state_mut().interaction.clear_focus() {
            self.request_frame();
        }
    }

    pub fn pointer_position(&self) -> Option<LogicalPoint> {
        self.state().interaction.pointer_position()
    }

    pub fn request_frame(&mut self) {
        self.state_mut().frame_requested = true
    }

    pub fn invalidate_all(&mut self) {
        let shared = self.state_mut();
        shared.frame_requested = true;
        shared.full_repaint = true;
    }
}

#[doc(hidden)]
impl Ui {
    pub fn text_run(&mut self, text: &str, style: TextStyle) -> TextRunId {
        self.platform_mut().text_run(text, style)
    }

    pub fn text_offset_at_position(
        &mut self,
        request: &paint::TextRequest,
        position: LogicalPoint,
    ) -> usize {
        self.platform_mut()
            .text_offset_at_position(request, position)
    }

    pub fn text_cursor_rect(
        &mut self,
        request: &paint::TextRequest,
        byte_offset: usize,
    ) -> LogicalRect {
        self.platform_mut().text_cursor_rect(request, byte_offset)
    }

    pub fn add_leaf(&mut self, item: Item, content: Content) -> NodeId {
        self.frame_mut().add_leaf(item, content)
    }

    pub fn open_container(&mut self, axis: Axis, container: ContainerConfig<'_>) -> NodeId {
        self.frame_mut().add_container(axis, container, None)
    }

    pub fn open_absolute_container(
        &mut self,
        axis: Axis,
        container: ContainerConfig<'_>,
        absolute: Absolute,
    ) -> NodeId {
        self.frame_mut()
            .add_container(axis, container, Some(absolute))
    }

    pub fn close_container(&mut self, node: NodeId) {
        self.frame_mut().close(node)
    }

    pub fn set_node_id(&mut self, node: NodeId, id: WidgetId) {
        self.frame_mut().set_id(node, id)
    }

    pub fn set_node_appearance(&mut self, node: NodeId, appearance: Style<'_>) {
        self.frame_mut().set_appearance(node, appearance)
    }

    fn state(&self) -> &UiState {
        // only used in context of render
        unsafe { self.state.as_ref() }
    }

    fn state_mut(&mut self) -> &mut UiState {
        // only used in context of render
        unsafe { self.state.as_mut() }
    }

    fn platform_mut(&mut self) -> &mut dyn PlatformImpl {
        // only used in context of render
        unsafe { self.platform.as_mut() }
    }

    fn frame_mut(&mut self) -> &mut graph::FrameGraph {
        &mut self.state_mut().frame
    }
}

fn begin_animation(
    ui: &mut Ui,
    id: WidgetId,
    initial: f32,
    advance: impl FnOnce(&mut animation::AnimationState),
) -> f32 {
    let animations = &mut ui.state_mut().animations;
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
    let timers = &mut ui.state_mut().timers;
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

pub struct UiState {
    frame: graph::FrameGraph,
    commands: CommandList,
    interaction: interact::InteractionState,
    geometry: graph::GeometryState,
    animations: Vec<animation::AnimationState>,
    timers: Vec<timer::TimerState>,
    frame_requested: bool,
    full_repaint: bool,
    screen: LogicalRect,
    physical_screen: PhysicalRect,
    scale_factor: f32,
    previous_commands: CommandList,
    differ: CommandListDiffer,
    previous_damage: Vec<PhysicalRect>,
    render_damage: Vec<PhysicalRect>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            frame: graph::FrameGraph::default(),
            commands: CommandList::default(),
            interaction: interact::InteractionState::default(),
            geometry: graph::GeometryState::default(),
            animations: Vec::new(),
            timers: Vec::new(),
            frame_requested: true,
            full_repaint: true,
            screen: LogicalRect::default(),
            physical_screen: PhysicalRect::default(),
            scale_factor: 1.0,
            previous_commands: CommandList::default(),
            differ: CommandListDiffer::default(),
            previous_damage: Vec::new(),
            render_damage: Vec::new(),
        }
    }
}

impl UiState {
    pub fn has_pending_redraw(&self) -> bool {
        self.frame_requested
            || !self.previous_damage.is_empty()
            || self
                .animations
                .iter()
                .any(animation::AnimationState::is_active)
    }

    pub fn next_timer_deadline(&self) -> Option<Duration> {
        self.timers
            .iter()
            .filter_map(timer::TimerState::deadline)
            .min()
    }

    pub fn request_frame(&mut self) {
        self.frame_requested = true;
    }

    pub fn invalidate_all(&mut self) {
        self.frame_requested = true;
        self.full_repaint = true;
    }

    pub fn set_command_diff_config(&mut self, config: CommandDiffConfig) {
        self.differ.set_config(config);
    }

    pub fn screen(&self) -> LogicalRect {
        self.screen
    }

    pub fn frame_graph_memory(&self) -> FrameGraphMemory {
        self.frame.memory()
    }
}

/// processes inputs and renders the final UI frame
///
/// `build` runs once per input so each event observes state changes from
/// earlier events. layout and interaction state are updated after every call,
/// while command diffing and platform rendering occur once after the last input
///
/// an empty input sequence builds once with [`Input::None`]. the return value is
/// from the final call to `build`
pub fn render<P: PlatformImpl + 'static, R>(
    platform: &mut P,
    state: &mut UiState,
    time: Duration,
    inputs: impl IntoIterator<Item = Input>,
    mut build: impl FnMut(&mut Ui) -> R,
) -> R {
    // refresh platform-dependent frame state
    let repaint_buffer = platform.repaint_buffer();
    let physical_screen = platform.screen();
    let scale_factor = platform.scale_factor();
    assert!(scale_factor.is_finite() && scale_factor > 0.0);
    if state.physical_screen != physical_screen || state.scale_factor != scale_factor {
        state.physical_screen = physical_screen;
        state.screen = physical_screen.to_logical(scale_factor);
        state.scale_factor = scale_factor;
        state.previous_damage.clear();
        state.invalidate_all();
    }

    // record every input against the state produced by the previous one
    let mut inputs = inputs.into_iter();
    state.frame_requested = false;
    let mut output = record(
        platform,
        state,
        time,
        inputs.next().unwrap_or_default(),
        &mut build,
    );
    for input in inputs {
        output = record(platform, state, time, input, &mut build);
    }

    // diff and render only the final recorded frame
    state.render_damage.clear();
    if std::mem::take(&mut state.full_repaint) {
        state.render_damage.push(state.physical_screen);
    } else {
        state
            .render_damage
            .extend_from_slice(state.differ.diff(&state.previous_commands, &state.commands));
    }
    let current_damage_len = state.render_damage.len();
    if repaint_buffer == RepaintBuffer::Swapped {
        state
            .render_damage
            .extend_from_slice(&state.previous_damage);
    }
    platform.render(&state.commands, &state.render_damage);
    state.previous_damage.clear();
    if repaint_buffer == RepaintBuffer::Swapped {
        state
            .previous_damage
            .extend_from_slice(&state.render_damage[..current_damage_len]);
    }
    std::mem::swap(&mut state.previous_commands, &mut state.commands);
    output
}

fn record<P: PlatformImpl + 'static, R>(
    platform: &mut P,
    state: &mut UiState,
    time: Duration,
    input: Input,
    build: impl FnOnce(&mut Ui) -> R,
) -> R {
    // reset transient data and begin input processing
    state.commands.clear();
    state.frame.begin(state.screen);
    for animation in &mut state.animations {
        animation.seen = false;
    }
    for timer in &mut state.timers {
        timer.seen = false;
    }
    state.interaction.begin_frame(&input, state.scale_factor);

    let output = {
        // expose state and platform only for the build callback
        let platform_ptr = NonNull::from(&mut *platform as &mut dyn PlatformImpl);
        let mut ui = Ui {
            state: NonNull::from(&mut *state),
            platform: platform_ptr,
            time,
            input,
        };
        build(&mut ui)
    };

    // resolve the frame and retain state needed by the next input
    let mut frame = std::mem::take(&mut state.frame);
    frame.finish(
        platform,
        &mut state.commands,
        &mut state.interaction,
        &mut state.geometry,
        state.scale_factor,
    );
    state.frame = frame;
    if state.interaction.end_frame(state.scale_factor) {
        state.frame_requested = true;
    }
    state.geometry.end_frame();
    state.animations.retain(|animation| animation.seen);
    state.timers.retain(|timer| timer.seen);
    output
}
