use std::{num::NonZeroU32, pin::Pin, rc::Rc, time::Instant};

use blit::{
    RepaintBuffer,
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    input::{Input, Key, KeyInput, Modifiers, PointerButton, ScrollPhase},
    keyboard::KeyboardRequest,
    paint::TextRequest,
    paint_list::PaintList,
    platform::PlatformImpl,
    resource::{ImageData, ImageId, StringData, StringId},
};
use blit_cpu::{PixelBuffer, Renderer, Scanline};
use blit_executor::{LocalExecutor, TaskId};
use softbuffer::{Context, Surface};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize as WindowSize, PhysicalPosition, PhysicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle},
    keyboard::{Key as WindowKey, NamedKey},
    window::{Window, WindowId},
};

use crate::{Application, Config, EventLoopProxy, RunError, pixel::DesktopBuffer};

pub enum Event<T> {
    Input(T),
    TaskReady(TaskId),
}

pub fn run<A: Application>(config: Config) -> Result<(), RunError> {
    let event_loop = EventLoop::<Event<A::Input>>::with_user_event().build()?;
    let context = Context::new(event_loop.owned_display_handle())?;
    let mut runner: Runner<A> = Runner {
        state: Some(State::Pending {
            config,
            input: EventLoopProxy {
                inner: event_loop.create_proxy(),
            },
        }),
        context,
        inputs: Vec::new(),
        cursor: None,
        modifiers: Modifiers::NONE,
        started_at: Instant::now(),
        error: None,
    };
    event_loop.run_app(&mut runner)?;
    runner.error.map_or(Ok(()), Err)
}

struct Runner<A: Application> {
    state: Option<State<A>>,
    context: Context<OwnedDisplayHandle>,
    inputs: Vec<Input>,
    cursor: Option<LogicalPoint>,
    modifiers: Modifiers,
    started_at: Instant,
    error: Option<RunError>,
}

enum State<A: Application> {
    Pending {
        config: Config,
        input: EventLoopProxy<A::Input>,
    },
    Active(Box<Active<A>>),
}

struct Active<A: Application> {
    app: A,
    executor: Pin<Box<LocalExecutor<A>>>,
    runtime: blit::Runtime<DesktopPlatform>,
    surface: Surface<OwnedDisplayHandle, Rc<Window>>,
    window: Rc<Window>,
}

impl<A: Application> Active<A> {
    fn resize(&mut self, size: PhysicalSize<u32>, scale_factor: f64) -> Result<(), RunError> {
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        self.surface.resize(width, height)?;
        self.runtime
            .platform()
            .renderer
            .buffer_mut()
            .resize(size.width as usize, size.height as usize);
        self.runtime
            .platform()
            .renderer
            .set_scale_factor(scale_factor as f32);
        self.runtime.refresh_screen();
        Ok(())
    }
}

impl<A: Application> Runner<A> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl Into<RunError>) {
        self.error = Some(error.into());
        event_loop.exit();
    }

    fn push_text(&mut self, text: &str) {
        self.inputs.extend(text.chars().map(Input::Text))
    }

    fn push_input(&mut self, input: Input) {
        if let Input::PointerMove {
            position,
            modifiers,
        } = input
            && let Some(Input::PointerMove {
                position: pending,
                modifiers: pending_modifiers,
            }) = self.inputs.last_mut()
        {
            *pending = position;
            *pending_modifiers = modifiers;
        } else if let Input::Scroll {
            position,
            delta_x,
            delta_y,
            modifiers,
            continuous,
            phase,
        } = input
            && let Some(Input::Scroll {
                position: pending_position,
                delta_x: pending_x,
                delta_y: pending_y,
                modifiers: pending_modifiers,
                continuous: pending_continuous,
                phase: pending_phase,
            }) = self.inputs.last_mut()
            && *pending_modifiers == modifiers
            && *pending_continuous == continuous
            && *pending_phase == phase
        {
            *pending_position = position;
            *pending_x += delta_x;
            *pending_y += delta_y;
        } else {
            self.inputs.push(input);
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let Some(State::Active(active)) = &mut self.state else {
            return;
        };
        let time = self.started_at.elapsed();
        let timer_due = active
            .runtime
            .next_timer_deadline()
            .is_some_and(|deadline| time >= deadline);
        if self.inputs.is_empty() && !active.runtime.has_pending_redraw() && !timer_due {
            return;
        }
        let mut buffer = match active.surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(error) => return self.fail(event_loop, error),
        };
        if buffer.len()
            != active.runtime.platform().renderer.buffer().width()
                * active.runtime.platform().renderer.buffer().height()
        {
            return;
        }
        if buffer.age() == 0 {
            active.runtime.invalidate_all();
        }
        active
            .runtime
            .platform()
            .renderer
            .buffer_mut()
            .set(&mut buffer);
        if !self.inputs.is_empty() {
            active
                .runtime
                .render_batch(time, self.inputs.drain(..), |ui| active.app.render(ui));
        } else if active.runtime.has_pending_redraw() || timer_due {
            active
                .runtime
                .render(time, Input::None, |ui| active.app.render(ui));
        }
        active.window.pre_present_notify();
        if let Err(error) = buffer.present() {
            self.fail(event_loop, error);
        }
    }
}

impl<A: Application> ApplicationHandler<Event<A::Input>> for Runner<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let (config, input) = match self.state.take() {
            Some(State::Pending { config, input }) => (config, input),
            state => {
                self.state = state;
                return;
            }
        };
        let attributes = Window::default_attributes()
            .with_title(config.title)
            .with_inner_size(WindowSize::new(config.width, config.height));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Rc::new(window),
            Err(error) => return self.fail(event_loop, error),
        };
        let size = window.inner_size();
        let size = PhysicalSize::new(size.width.max(1), size.height.max(1));
        let surface = match Surface::new(&self.context, window.clone()) {
            Ok(surface) => surface,
            Err(error) => return self.fail(event_loop, error),
        };
        let platform = DesktopPlatform {
            renderer: Renderer::new(
                DesktopBuffer::new(size.width as usize, size.height as usize),
                config.renderer,
            )
            .with_scale_factor(window.scale_factor() as f32)
            .strategy(Scanline::default()),
            window: window.clone(),
            ime_allowed: false,
            ime_requested: false,
        };
        let mut runtime = blit::Runtime::new(platform);
        let wake = input.inner.clone();
        let executor = Box::pin(LocalExecutor::new(move |task| {
            let _ = wake.send_event(Event::TaskReady(task));
        }));
        // safety: executor remains pinned and is dropped after app
        let ops = unsafe { executor.as_ref().ops() };
        let app = A::new(*runtime.erased_platform(), input, ops);
        let scale_factor = window.scale_factor();
        let mut active = Box::new(Active {
            app,
            executor,
            runtime,
            surface,
            window,
        });
        if let Err(error) = active.resize(size, scale_factor) {
            return self.fail(event_loop, error);
        }
        active.window.request_redraw();
        self.state = Some(State::Active(active));
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: Event<A::Input>) {
        let Some(State::Active(active)) = &mut self.state else {
            return;
        };
        let request_frame = match event {
            Event::Input(input) => {
                active.app.input(input);
                true
            }
            Event::TaskReady(task) => active.executor.as_ref().run(&mut active.app, task),
        };
        if request_frame {
            active.runtime.request_frame();
            active.window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(State::Active(active)) = &mut self.state else {
            return;
        };
        if active.window.id() != window_id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Err(error) = active.resize(size, active.window.scale_factor()) {
                    self.fail(event_loop, error);
                } else {
                    active.window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Err(error) = active.resize(active.window.inner_size(), scale_factor) {
                    self.fail(event_loop, error);
                } else {
                    active.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            WindowEvent::ModifiersChanged(state) => {
                let state = state.state();
                self.modifiers = Modifiers::new(
                    state.shift_key(),
                    state.control_key(),
                    state.alt_key(),
                    state.super_key(),
                );
            }
            WindowEvent::CursorMoved { position, .. } => {
                let position = logical_position(position, active.window.scale_factor());
                active.window.request_redraw();
                self.cursor = Some(position);
                self.push_input(Input::PointerMove {
                    position,
                    modifiers: self.modifiers,
                });
            }
            WindowEvent::CursorLeft { .. } => {
                active.window.request_redraw();
                self.cursor = None;
                self.inputs.push(Input::PointerLeave);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(position) = self.cursor {
                    let button = match button {
                        MouseButton::Left => PointerButton::Primary,
                        MouseButton::Right => PointerButton::Secondary,
                        MouseButton::Middle => PointerButton::Middle,
                        MouseButton::Back => PointerButton::Back,
                        MouseButton::Forward => PointerButton::Forward,
                        MouseButton::Other(button) => PointerButton::Other(button),
                    };
                    active.window.request_redraw();
                    self.push_input(if state == ElementState::Pressed {
                        Input::PointerDown {
                            position,
                            button,
                            modifiers: self.modifiers,
                        }
                    } else {
                        Input::PointerUp {
                            position,
                            button,
                            modifiers: self.modifiers,
                            leave: false,
                        }
                    });
                }
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                let position = self.cursor.unwrap_or_else(|| {
                    let size = active.window.inner_size();
                    let scale = active.window.scale_factor() as f32;
                    LogicalPoint {
                        x: size.width as f32 / scale / 2.0,
                        y: size.height as f32 / scale / 2.0,
                    }
                });
                let (delta_x, delta_y, continuous) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (-x * 40.0, -y * 40.0, false),
                    MouseScrollDelta::PixelDelta(delta) => (
                        (-delta.x / active.window.scale_factor()) as f32,
                        (-delta.y / active.window.scale_factor()) as f32,
                        true,
                    ),
                };
                let phase = match phase {
                    TouchPhase::Started => ScrollPhase::Started,
                    TouchPhase::Moved => ScrollPhase::Moved,
                    TouchPhase::Ended | TouchPhase::Cancelled => ScrollPhase::Ended,
                };
                active.window.request_redraw();
                self.push_input(Input::Scroll {
                    position,
                    delta_x,
                    delta_y,
                    modifiers: self.modifiers,
                    continuous,
                    phase,
                });
            }
            WindowEvent::KeyboardInput { event, .. } => {
                active.window.request_redraw();
                let key = match event.logical_key {
                    WindowKey::Character(character) => character.chars().next().map(Key::Character),
                    WindowKey::Named(NamedKey::Backspace) => Some(Key::Backspace),
                    WindowKey::Named(NamedKey::Delete) => Some(Key::Delete),
                    WindowKey::Named(NamedKey::ArrowLeft) => Some(Key::ArrowLeft),
                    WindowKey::Named(NamedKey::ArrowRight) => Some(Key::ArrowRight),
                    WindowKey::Named(NamedKey::ArrowUp) => Some(Key::ArrowUp),
                    WindowKey::Named(NamedKey::ArrowDown) => Some(Key::ArrowDown),
                    WindowKey::Named(NamedKey::Enter) => Some(Key::Enter),
                    WindowKey::Named(NamedKey::Tab) => Some(Key::Tab),
                    WindowKey::Named(NamedKey::Escape) => Some(Key::Escape),
                    WindowKey::Named(NamedKey::Home) => Some(Key::Home),
                    WindowKey::Named(NamedKey::End) => Some(Key::End),
                    WindowKey::Named(NamedKey::PageUp) => Some(Key::PageUp),
                    WindowKey::Named(NamedKey::PageDown) => Some(Key::PageDown),
                    WindowKey::Named(NamedKey::Insert) => Some(Key::Insert),
                    WindowKey::Named(key) => match key {
                        NamedKey::F1 => Some(Key::Function(1)),
                        NamedKey::F2 => Some(Key::Function(2)),
                        NamedKey::F3 => Some(Key::Function(3)),
                        NamedKey::F4 => Some(Key::Function(4)),
                        NamedKey::F5 => Some(Key::Function(5)),
                        NamedKey::F6 => Some(Key::Function(6)),
                        NamedKey::F7 => Some(Key::Function(7)),
                        NamedKey::F8 => Some(Key::Function(8)),
                        NamedKey::F9 => Some(Key::Function(9)),
                        NamedKey::F10 => Some(Key::Function(10)),
                        NamedKey::F11 => Some(Key::Function(11)),
                        NamedKey::F12 => Some(Key::Function(12)),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(key) = key {
                    self.push_input(Input::Key(KeyInput {
                        key,
                        modifiers: self.modifiers,
                        pressed: event.state == ElementState::Pressed,
                        repeat: event.repeat,
                    }));
                }
                if event.state == ElementState::Pressed
                    && !self.modifiers.control()
                    && !self.modifiers.alt()
                    && !self.modifiers.super_key()
                    && let Some(text) = event.text
                {
                    self.push_text(&text);
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                active.window.request_redraw();
                self.push_text(&text);
            }
            WindowEvent::Touch(touch) => {
                let position = logical_position(touch.location, active.window.scale_factor());
                active.window.request_redraw();
                let input = match touch.phase {
                    TouchPhase::Started => Input::PointerDown {
                        position,
                        button: PointerButton::Primary,
                        modifiers: self.modifiers,
                    },
                    TouchPhase::Moved => Input::PointerMove {
                        position,
                        modifiers: self.modifiers,
                    },
                    TouchPhase::Ended | TouchPhase::Cancelled => Input::PointerUp {
                        position,
                        button: PointerButton::Primary,
                        modifiers: self.modifiers,
                        leave: true,
                    },
                };
                self.push_input(input);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(State::Active(active)) = &self.state else {
            return;
        };
        let now = self.started_at.elapsed();
        if active.runtime.has_pending_redraw()
            || active
                .runtime
                .next_timer_deadline()
                .is_some_and(|deadline| deadline <= now)
            || !self.inputs.is_empty()
        {
            active.window.request_redraw();
            event_loop.set_control_flow(ControlFlow::Wait);
        } else if let Some(deadline) = active.runtime.next_timer_deadline() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.started_at + deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

fn logical_position(position: PhysicalPosition<f64>, scale_factor: f64) -> LogicalPoint {
    LogicalPoint {
        x: (position.x / scale_factor) as f32,
        y: (position.y / scale_factor) as f32,
    }
}

struct DesktopPlatform {
    renderer: Renderer<DesktopBuffer, Scanline>,
    window: Rc<Window>,
    ime_allowed: bool,
    ime_requested: bool,
}

impl PlatformImpl for DesktopPlatform {
    fn render(&mut self, paint: &PaintList, damage: &[PhysicalRect]) {
        if self.ime_allowed != self.ime_requested {
            self.window.set_ime_allowed(self.ime_requested);
            self.ime_allowed = self.ime_requested;
        }
        self.ime_requested = false;
        self.renderer.render(paint, damage)
    }

    fn screen(&mut self) -> PhysicalRect {
        self.renderer.screen()
    }

    fn scale_factor(&mut self) -> f32 {
        self.renderer.scale_factor()
    }

    fn repaint_buffer(&self) -> RepaintBuffer {
        RepaintBuffer::Swapped
    }

    fn create_image(&mut self, data: ImageData) -> ImageId {
        self.renderer.create_image(data)
    }

    fn drop_image(&mut self, image: ImageId) {
        self.renderer.drop_image(image)
    }

    fn create_string(&mut self, string: StringData) -> StringId {
        self.renderer.create_string(string)
    }

    fn drop_string(&mut self, string: StringId) {
        self.renderer.drop_string(string)
    }

    fn string(&self, string: StringId) -> &str {
        self.renderer.string(string)
    }

    fn text_offset_at_position(&mut self, request: &TextRequest, position: LogicalPoint) -> usize {
        self.renderer.text_offset_at_position(request, position)
    }

    fn measure_text(&mut self, request: &TextRequest) -> LogicalSize {
        self.renderer.measure_text(request)
    }

    fn measure_text_height(&mut self, request: &TextRequest) -> f32 {
        self.renderer.measure_text_height(request)
    }

    fn text_cursor_rect(&mut self, request: &TextRequest, byte_offset: usize) -> LogicalRect {
        self.renderer.text_cursor_rect(request, byte_offset)
    }

    fn show_keyboard(&mut self, _: &KeyboardRequest<'_>) {
        self.ime_requested = true
    }
}
