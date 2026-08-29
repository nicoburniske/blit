pub mod animation;
pub mod color;
pub mod command_list;
pub mod container;
mod frame;
pub mod geometry;
pub mod image;
pub mod input;
pub mod interact;
pub mod layout;
mod macros;
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
use container::{Container, ContainerConfig, LayerId, Slot};
use geometry::{LogicalPoint, LogicalRect, Scale2};
use image::{ImageData, ImageHandle};
use input::Input;
use interact::{Interaction, Sense, WidgetId};
use layout::LayoutResolution;
use node::{Content, NodeId};
use renderer::{RenderGeometry, Renderer};
use repaint::Repaint;
use style::Style;
use text::{TextRunId, TextStyle};

pub struct Ui {
    state: NonNull<UiState>,
    renderer: NonNull<dyn Renderer>,
    time: Duration,
    input: Input,
    zoom: f32,
}

impl Ui {
    /// restores damaged screen pixels to the render target's default value before painting
    pub fn clear(&mut self) {
        self.frame_mut().clear()
    }

    pub fn layout<L: layout::Layout>(&mut self, layout: L) -> Container<'_, '_, L> {
        Container::new(self, layout)
    }

    /// declares a paint layer rooted at the current container
    pub fn layer(&mut self) -> LayerId {
        self.frame_mut().add_layer()
    }

    /// returns geometry resolved before the current render callback
    ///
    /// nodes declared in this callback become available to subsequent callbacks
    pub fn geometry(&self, id: WidgetId) -> Option<LogicalRect> {
        self.state().geometry.get(id)
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        self.renderer_mut().create_image(data)
    }

    pub fn screen(&self) -> LogicalRect {
        self.state().screen
    }

    pub fn layout_resolution(&self) -> LayoutResolution {
        self.state().layout_resolution
    }

    pub fn input(&self) -> &Input {
        &self.input
    }

    /// evaluates the current input against geometry resolved before this callback
    pub fn interact(&mut self, id: WidgetId, sense: Sense) -> Interaction {
        let interaction = self.state_mut().interaction.response(id, sense);
        if interaction.activated || interaction.deactivated || interaction.clicked {
            self.request_frame();
        }
        interaction
    }

    pub fn time(&self) -> Duration {
        self.time
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// changes zoom on the next frame
    pub fn set_zoom(&mut self, zoom: f32) {
        if self
            .state()
            .render_geometry
            .is_none_or(|geometry| geometry.supports_zoom)
        {
            self.state_mut().set_zoom(zoom)
        }
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
        animation::AnimationState::update(self, id, target, |animation| {
            animation.advance(target, duration, easing, time)
        })
    }

    /// animates a repeating value from `0.0` up to `1.0`, keyed by `id`
    ///
    /// zero duration stops the loop and resets the value
    pub fn animate_loop(&mut self, id: WidgetId, duration: Duration, easing: Easing) -> f32 {
        let time = self.time;
        animation::AnimationState::update(self, id, 0.0, |animation| {
            animation.advance_loop(duration, easing, time)
        })
    }

    /// returns `true` once when `duration` has elapsed for `id`
    ///
    /// the timer starts on its first call and is removed when it is not called
    /// during a frame. [`UiState::next_timer_deadline`] reports when the next
    /// timer needs a frame
    pub fn timer(&mut self, id: WidgetId, duration: Duration) -> bool {
        timer::TimerState::update(self, id, duration, None)
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
        timer::TimerState::update(self, id, duration, Some(duration))
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

    pub fn add_leaf(&mut self, slot: Slot, content: Content<'_>) -> NodeId {
        self.frame_mut().add_leaf(slot, content)
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
        let states = &mut self.state_mut().transitions;
        match states.binary_search_by_key(&id, |state| state.id) {
            Ok(index) => states[index].begin(node, transition),
            Err(index) => {
                states.insert(index, frame::TransitionState::new(id, node, transition));
            }
        }
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

    fn frame_mut(&mut self) -> &mut frame::FrameGraph {
        &mut self.state_mut().frame
    }
}

/// retained frame graph memory after its buffers have grown
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameGraphMemory {
    pub node_size: usize,
    pub node_capacity: usize,
    pub heap_bytes: usize,
}

pub struct UiState {
    frame: frame::FrameGraph,
    commands: CommandList,
    interaction: interact::InteractionState,
    geometry: frame::GeometryState,
    animations: Vec<animation::AnimationState>,
    transitions: Vec<frame::TransitionState>,
    timers: Vec<timer::TimerState>,
    frame_requested: bool,
    full_repaint: bool,
    screen: LogicalRect,
    render_geometry: Option<RenderGeometry>,
    zoom: f32,
    layout_resolution: LayoutResolution,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            frame: frame::FrameGraph::default(),
            commands: CommandList::default(),
            interaction: interact::InteractionState::default(),
            geometry: frame::GeometryState::default(),
            animations: Vec::new(),
            transitions: Vec::new(),
            timers: Vec::new(),
            frame_requested: true,
            full_repaint: true,
            screen: LogicalRect::default(),
            render_geometry: None,
            zoom: 1.0,
            layout_resolution: LayoutResolution::Continuous,
        }
    }
}

impl UiState {
    pub fn has_pending_redraw(&self) -> bool {
        self.frame_requested
            || self
                .animations
                .iter()
                .any(animation::AnimationState::is_active)
            || self
                .transitions
                .iter()
                .any(frame::TransitionState::is_active)
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

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        assert!(zoom.is_finite() && zoom > 0.0);
        if self.zoom != zoom {
            self.zoom = zoom;
            self.render_geometry = None;
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
    let geometry = renderer.geometry();
    let zoom = if geometry.supports_zoom {
        state.zoom
    } else {
        1.0
    };
    let scale = geometry.physical_per_logical.zoom(zoom);
    if state.render_geometry != Some(geometry) {
        assert!(scale.x.is_finite() && scale.x > 0.0);
        assert!(scale.y.is_finite() && scale.y > 0.0);
        renderer.set_scale(scale);
        state.render_geometry = Some(geometry);
        state.layout_resolution = geometry.layout_resolution;
        if let LayoutResolution::Discrete { step } = &mut state.layout_resolution {
            step.width /= zoom;
            step.height /= zoom;
        }
        state.screen = geometry.physical_bounds.to_logical(scale);
        state.full_repaint = true;
    }
    // record every input against the state produced by the previous one
    let mut inputs = inputs.into_iter();
    state.frame_requested = false;
    record(
        renderer,
        state,
        scale,
        zoom,
        time,
        inputs.next().unwrap_or_default(),
        &mut render,
    );
    for input in inputs {
        record(renderer, state, scale, zoom, time, input, &mut render);
    }

    // repaint only the final recorded frame
    if std::mem::take(&mut state.full_repaint) {
        repaint.invalidate();
    }
    repaint.render(renderer, &mut state.commands, geometry.physical_bounds);
}

fn record<P: Renderer>(
    renderer: &mut P,
    state: &mut UiState,
    scale: Scale2,
    zoom: f32,
    time: Duration,
    input: Input,
    render: impl FnOnce(&mut Ui),
) {
    // reset transient data and begin input processing
    state.commands.clear();
    state.frame.begin(state.screen, state.layout_resolution);
    for animation in &mut state.animations {
        animation.seen = false;
    }
    for transition in &mut state.transitions {
        transition.seen = false;
    }
    for timer in &mut state.timers {
        timer.seen = false;
    }
    state.interaction.begin_frame(&input);

    {
        let renderer = NonNull::from(&mut *renderer as &mut (dyn Renderer + '_));
        // safety: `Ui` only borrows this pointer for the render callback
        let renderer: NonNull<dyn Renderer> = unsafe { std::mem::transmute(renderer) };
        let mut ui = Ui {
            state: NonNull::from(&mut *state),
            renderer,
            time,
            input,
            zoom,
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
        scale,
    );
    state.frame = frame;
    if state.interaction.end_frame() {
        state.frame_requested = true;
    }
    state.geometry.end_frame();
    state.animations.retain(|animation| animation.seen);
    state.transitions.retain(|transition| transition.seen);
    state.timers.retain(|timer| timer.seen);
}
