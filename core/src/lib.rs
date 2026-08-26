pub mod animation;
mod builder;
pub mod color;
pub mod command_list;
pub mod container;
pub mod geometry;
mod graph;
pub mod image;
pub mod input;
pub mod interact;
pub mod layout;
pub mod node;
pub mod renderer;
pub mod repaint;
pub mod style;
#[cfg(test)]
mod test;
pub mod text;
mod timer;
pub mod widget;

use std::{ptr::NonNull, time::Duration};

use animation::Easing;
use command_list::CommandList;
use container::{Container, ContainerConfig, Item};
use geometry::{LogicalPoint, LogicalRect, PhysicalRect};
use image::{ImageData, ImageHandle};
use input::Input;
use interact::{Interaction, Sense, WidgetId};
use node::{Content, NodeId};
use renderer::Renderer;
use repaint::Repaint;
use style::Style;
use text::{TextRunId, TextStyle};

pub struct Ui {
    state: NonNull<UiState>,
    renderer: NonNull<dyn Renderer>,
    time: Duration,
    input: Input,
    scale_factor: f32,
}

impl Ui {
    /// restores damaged screen pixels to the render target's default value before painting
    pub fn clear(&mut self) {
        self.frame_mut().clear()
    }

    pub fn layout<L: layout::Layout>(&mut self, layout: L) -> Container<'_, '_, L> {
        Container::new(self, layout)
    }

    pub fn geometry(&self, id: WidgetId) -> Option<LogicalRect> {
        self.state().geometry.get(id)
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        self.renderer_mut().create_image(data)
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

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// changes the scale factor on the next frame
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.state_mut().set_scale_factor(scale_factor)
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
        self.state_mut().request_frame()
    }

    pub fn invalidate_all(&mut self) {
        self.state_mut().invalidate_all()
    }
}

impl Ui {
    pub fn text_run(&mut self, text: &str, style: TextStyle) -> TextRunId {
        self.renderer_mut().text_run(text, style)
    }

    pub fn text_offset_at_position(
        &mut self,
        request: &text::TextRequest,
        position: LogicalPoint,
    ) -> usize {
        self.renderer_mut()
            .text_offset_at_position(request, position)
    }

    pub fn text_cursor_rect(
        &mut self,
        request: &text::TextRequest,
        byte_offset: usize,
    ) -> LogicalRect {
        self.renderer_mut().text_cursor_rect(request, byte_offset)
    }

    pub fn add_leaf(&mut self, item: Item, content: Content<'_>) -> NodeId {
        self.frame_mut().add_leaf(item, content)
    }

    pub fn begin_layout_item(&self) -> NodeId {
        self.state().frame.begin_layout_item()
    }

    pub fn finish_layout_item<L: layout::Layout>(
        &mut self,
        parent: NodeId,
        child: NodeId,
        item: L::Item,
    ) {
        self.frame_mut()
            .finish_layout_item::<L>(parent, child, item)
    }

    pub fn open_layout<L: layout::Layout>(
        &mut self,
        layout: L,
        container: ContainerConfig<'_>,
    ) -> NodeId {
        let id = container.id;
        let transition = container.transition;
        let node = self.frame_mut().add_container(layout, container);
        if let (Some(id), Some(transition)) = (id, transition) {
            self.set_node_transition(node, id, transition);
        }
        node
    }

    pub fn close_container(&mut self, node: NodeId) {
        self.frame_mut().close(node)
    }

    pub fn set_node_id(&mut self, node: NodeId, id: WidgetId) {
        self.frame_mut().set_id(node, id)
    }

    pub fn set_node_style(&mut self, node: NodeId, style: Style<'_>) {
        self.frame_mut().set_style(node, style)
    }

    pub fn set_node_transition(
        &mut self,
        node: NodeId,
        id: WidgetId,
        transition: animation::Transition,
    ) {
        let index = if let Some(index) = self
            .state()
            .transitions
            .iter()
            .position(|state| state.id == id)
        {
            index
        } else {
            let states = &mut self.state_mut().transitions;
            states.push(graph::TransitionState::new(id));
            states.len() - 1
        };
        let parent = self.frame_mut().transition_parent(node);
        self.state_mut().transitions[index].begin(node, parent, transition);
    }

    fn state(&self) -> &UiState {
        // only used in context of render
        unsafe { self.state.as_ref() }
    }

    fn state_mut(&mut self) -> &mut UiState {
        // only used in context of render
        unsafe { self.state.as_mut() }
    }

    fn renderer_mut(&mut self) -> &mut dyn Renderer {
        // only used in context of render
        unsafe { self.renderer.as_mut() }
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
    transitions: Vec<graph::TransitionState>,
    timers: Vec<timer::TimerState>,
    frame_requested: bool,
    full_repaint: bool,
    screen: LogicalRect,
    physical_screen: PhysicalRect,
    scale_factor: f32,
    scale_factor_changed: bool,
}

impl UiState {
    pub fn new(physical_screen: PhysicalRect, scale_factor: f32) -> Self {
        assert!(scale_factor.is_finite() && scale_factor > 0.0);
        Self {
            frame: graph::FrameGraph::default(),
            commands: CommandList::default(),
            interaction: interact::InteractionState::default(),
            geometry: graph::GeometryState::default(),
            animations: Vec::new(),
            transitions: Vec::new(),
            timers: Vec::new(),
            frame_requested: true,
            full_repaint: true,
            screen: physical_screen.to_logical(scale_factor),
            physical_screen,
            scale_factor,
            scale_factor_changed: true,
        }
    }

    pub fn has_pending_redraw(&self) -> bool {
        self.frame_requested
            || self
                .animations
                .iter()
                .any(animation::AnimationState::is_active)
            || self
                .transitions
                .iter()
                .any(graph::TransitionState::is_active)
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

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub fn set_screen(&mut self, physical_screen: PhysicalRect) {
        if self.physical_screen != physical_screen {
            self.physical_screen = physical_screen;
            self.screen = physical_screen.to_logical(self.scale_factor);
            self.invalidate_all();
        }
    }

    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        assert!(scale_factor.is_finite() && scale_factor > 0.0);
        if self.scale_factor != scale_factor {
            self.scale_factor = scale_factor;
            self.scale_factor_changed = true;
            self.frame_requested = true;
        }
    }

    pub fn invalidate_all(&mut self) {
        self.frame_requested = true;
        self.full_repaint = true;
    }

    pub fn frame_graph_memory(&self) -> FrameGraphMemory {
        self.frame.memory()
    }
}

/// processes inputs and renders the final UI frame
///
/// `render` runs once per input so each event observes state changes from
/// earlier events. layout and interaction state are updated after every call,
/// while only the final command list is repainted after the last input
///
/// an empty input sequence renders once with [`Input::None`]
///
/// the same renderer and repaint policy must be used for the lifetime of the
/// [`UiState`]
pub fn render<P: Renderer, R: Repaint>(
    renderer: &mut P,
    state: &mut UiState,
    repaint: &mut R,
    time: Duration,
    inputs: impl IntoIterator<Item = Input>,
    mut render: impl FnMut(&mut Ui),
) {
    let scale_factor = state.scale_factor;
    if std::mem::take(&mut state.scale_factor_changed) {
        renderer.set_scale_factor(scale_factor);
        state.screen = state.physical_screen.to_logical(scale_factor);
        state.full_repaint = true;
    }

    // record every input against the state produced by the previous one
    let mut inputs = inputs.into_iter();
    state.frame_requested = false;
    record(
        renderer,
        state,
        scale_factor,
        time,
        inputs.next().unwrap_or_default(),
        &mut render,
    );
    for input in inputs {
        record(renderer, state, scale_factor, time, input, &mut render);
    }

    // repaint only the final recorded frame
    if std::mem::take(&mut state.full_repaint) {
        repaint.invalidate();
    }
    repaint.render(renderer, &mut state.commands, state.physical_screen);
}

fn record<P: Renderer>(
    renderer: &mut P,
    state: &mut UiState,
    scale_factor: f32,
    time: Duration,
    input: Input,
    render: impl FnOnce(&mut Ui),
) {
    // reset transient data and begin input processing
    state.commands.clear();
    state.frame.begin(state.screen);
    for animation in &mut state.animations {
        animation.seen = false;
    }
    for transition in &mut state.transitions {
        transition.seen = false;
    }
    for timer in &mut state.timers {
        timer.seen = false;
    }
    state.interaction.begin_frame(&input, scale_factor);

    {
        let renderer = NonNull::from(&mut *renderer as &mut (dyn Renderer + '_));
        // safety: `Ui` only borrows this pointer for the render callback
        let renderer: NonNull<dyn Renderer> = unsafe { std::mem::transmute(renderer) };
        let mut ui = Ui {
            state: NonNull::from(&mut *state),
            renderer,
            time,
            input,
            scale_factor,
        };
        render(&mut ui);
    }

    // resolve the frame and retain state needed by the next input
    let mut frame = std::mem::take(&mut state.frame);
    frame.finish(
        renderer,
        &mut state.commands,
        &mut state.interaction,
        &mut state.geometry,
        &mut state.transitions,
        time,
        scale_factor,
    );
    state.frame = frame;
    if state.interaction.end_frame(scale_factor) {
        state.frame_requested = true;
    }
    state.geometry.end_frame();
    state.animations.retain(|animation| animation.seen);
    state.transitions.retain(|transition| transition.seen);
    state.timers.retain(|timer| timer.seen);
}
