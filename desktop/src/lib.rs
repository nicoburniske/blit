#![feature(portable_simd)]

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("blit-desktop currently supports Linux and macOS only");

mod pixel;

use std::{error::Error, fmt, num::NonZeroU32, rc::Rc, time::Instant};

use blit::{
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    input::{Input, Key, KeyInput, Modifiers, PointerButton, ScrollPhase},
    keyboard::KeyboardRequest,
    paint::TextRequest,
    paint_list::PaintList,
    platform::{Platform, PlatformImpl},
    resource::{ImageData, ImageId, StringData, StringId},
    RepaintBuffer, Ui,
};
use blit_cpu::{PixelBuffer, Renderer, RendererConfig, Scanline};
use pixel::DesktopBuffer;
use softbuffer::{Context, Surface};
pub use winit::event_loop::EventLoopProxy;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize as WindowSize, PhysicalPosition, PhysicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle},
    keyboard::{Key as WindowKey, NamedKey},
    window::{Window, WindowId},
};

pub struct Config {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub renderer: RendererConfig,
}

pub trait Application: Sized + 'static {
    type Input: Send + 'static;

    fn new(platform: Platform, input: EventLoopProxy<Self::Input>) -> Self;

    fn input(&mut self, input: Self::Input);

    fn render(&mut self, ui: &mut Ui);
}

pub fn run<A: Application>(config: Config) -> Result<(), RunError> {
    let event_loop = EventLoop::<A::Input>::with_user_event().build()?;
    let context = Context::new(event_loop.owned_display_handle())?;
    let mut runner: Runner<A> = Runner {
        config: Some(config),
        context,
        input: Some(event_loop.create_proxy()),
        window: None,
        surface: None,
        runtime: None,
        app: None,
        inputs: Vec::new(),
        cursor: None,
        modifiers: Modifiers::NONE,
        started_at: Instant::now(),
        error: None,
    };
    event_loop.run_app(&mut runner)?;
    runner.error.map_or(Ok(()), Err)
}

#[derive(Debug)]
pub enum RunError {
    EventLoop(winit::error::EventLoopError),
    Window(winit::error::OsError),
    Surface(softbuffer::SoftBufferError),
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventLoop(error) => error.fmt(formatter),
            Self::Window(error) => error.fmt(formatter),
            Self::Surface(error) => error.fmt(formatter),
        }
    }
}

impl Error for RunError {}

impl From<winit::error::EventLoopError> for RunError {
    fn from(error: winit::error::EventLoopError) -> Self { Self::EventLoop(error) }
}

impl From<winit::error::OsError> for RunError {
    fn from(error: winit::error::OsError) -> Self { Self::Window(error) }
}

impl From<softbuffer::SoftBufferError> for RunError {
    fn from(error: softbuffer::SoftBufferError) -> Self { Self::Surface(error) }
}

struct Runner<A: Application> {
    config: Option<Config>,
    context: Context<OwnedDisplayHandle>,
    input: Option<EventLoopProxy<A::Input>>,
    window: Option<Rc<Window>>,
    surface: Option<Surface<OwnedDisplayHandle, Rc<Window>>>,
    runtime: Option<blit::Runtime<DesktopPlatform>>,
    app: Option<A>,
    inputs: Vec<Input>,
    cursor: Option<LogicalPoint>,
    modifiers: Modifiers,
    started_at: Instant,
    error: Option<RunError>,
}

impl<A: Application> Runner<A> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl Into<RunError>) {
        self.error = Some(error.into());
        event_loop.exit();
    }

    fn resize(&mut self, size: PhysicalSize<u32>, scale_factor: f64) -> Result<(), RunError> {
        let (Some(width), Some(height)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return Ok(());
        };
        self.surface.as_mut().unwrap().resize(width, height)?;
        let runtime = self.runtime.as_mut().unwrap();
        runtime.platform().renderer.buffer_mut().resize(size.width as usize, size.height as usize);
        runtime.platform().renderer.set_scale_factor(scale_factor as f32);
        runtime.refresh_screen();
        Ok(())
    }

    fn push_text(&mut self, text: &str) { self.inputs.extend(text.chars().map(Input::Text)) }

    fn push_input(&mut self, input: Input) {
        if let Input::PointerMove { position, modifiers } = input
            && let Some(Input::PointerMove { position: pending, modifiers: pending_modifiers }) =
                self.inputs.last_mut()
        {
            *pending = position;
            *pending_modifiers = modifiers;
        } else if let Input::Scroll { position, delta_x, delta_y, modifiers, continuous, phase } = input
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
        let (Some(window), Some(surface), Some(runtime), Some(app)) =
            (&self.window, &mut self.surface, &mut self.runtime, &mut self.app)
        else {
            return;
        };
        let time = self.started_at.elapsed();
        let timer_due = runtime.next_timer_deadline().is_some_and(|deadline| time >= deadline);
        if self.inputs.is_empty() && !runtime.has_pending_redraw() && !timer_due {
            return;
        }
        let mut buffer = match surface.buffer_mut() {
            Ok(buffer) => buffer,
            Err(error) => return self.fail(event_loop, error),
        };
        if buffer.len()
            != runtime.platform().renderer.buffer().width() * runtime.platform().renderer.buffer().height()
        {
            return;
        }
        if buffer.age() == 0 {
            runtime.invalidate_all();
        }
        runtime.platform().renderer.buffer_mut().set(&mut buffer);
        if !self.inputs.is_empty() {
            runtime.render_batch(time, self.inputs.drain(..), |ui| app.render(ui));
        } else if runtime.has_pending_redraw() || timer_due {
            runtime.render(time, Input::None, |ui| app.render(ui));
        }
        window.pre_present_notify();
        if let Err(error) = buffer.present() {
            self.fail(event_loop, error);
        }
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl<A: Application> ApplicationHandler<A::Input> for Runner<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let config = self.config.take().unwrap();
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
        let app = A::new(*runtime.erased_platform(), self.input.take().unwrap());
        self.window = Some(window);
        self.surface = Some(surface);
        self.runtime = Some(runtime);
        self.app = Some(app);
        if let Err(error) = self.resize(size, self.window.as_ref().unwrap().scale_factor()) {
            return self.fail(event_loop, error);
        }
        self.request_redraw();
    }

    fn user_event(&mut self, _: &ActiveEventLoop, input: A::Input) {
        let Some(app) = &mut self.app else { return };
        app.input(input);
        self.runtime.as_mut().unwrap().request_frame();
        self.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let Some(window) = &self.window else { return };
        if window.id() != window_id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Err(error) = self.resize(size, window.scale_factor()) {
                    self.fail(event_loop, error);
                } else {
                    self.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Err(error) = self.resize(window.inner_size(), scale_factor) {
                    self.fail(event_loop, error);
                } else {
                    self.request_redraw();
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
                let position = logical_position(position, window.scale_factor());
                self.cursor = Some(position);
                self.push_input(Input::PointerMove { position, modifiers: self.modifiers });
                self.request_redraw();
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.inputs.push(Input::PointerLeave);
                self.request_redraw();
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
                    self.push_input(if state == ElementState::Pressed {
                        Input::PointerDown { position, button, modifiers: self.modifiers }
                    } else {
                        Input::PointerUp { position, button, modifiers: self.modifiers, leave: false }
                    });
                    self.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                let position = self.cursor.unwrap_or_else(|| {
                    let size = window.inner_size();
                    let scale = window.scale_factor() as f32;
                    LogicalPoint { x: size.width as f32 / scale / 2.0, y: size.height as f32 / scale / 2.0 }
                });
                let (delta_x, delta_y, continuous) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (-x * 40.0, -y * 40.0, false),
                    MouseScrollDelta::PixelDelta(delta) => (
                        (-delta.x / window.scale_factor()) as f32,
                        (-delta.y / window.scale_factor()) as f32,
                        true,
                    ),
                };
                let phase = match phase {
                    TouchPhase::Started => ScrollPhase::Started,
                    TouchPhase::Moved => ScrollPhase::Moved,
                    TouchPhase::Ended | TouchPhase::Cancelled => ScrollPhase::Ended,
                };
                self.push_input(Input::Scroll {
                    position,
                    delta_x,
                    delta_y,
                    modifiers: self.modifiers,
                    continuous,
                    phase,
                });
                self.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
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
                self.request_redraw();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.push_text(&text);
                self.request_redraw();
            }
            WindowEvent::Touch(touch) => {
                let position = logical_position(touch.location, window.scale_factor());
                let input = match touch.phase {
                    TouchPhase::Started => Input::PointerDown {
                        position,
                        button: PointerButton::Primary,
                        modifiers: self.modifiers,
                    },
                    TouchPhase::Moved => Input::PointerMove { position, modifiers: self.modifiers },
                    TouchPhase::Ended | TouchPhase::Cancelled => Input::PointerUp {
                        position,
                        button: PointerButton::Primary,
                        modifiers: self.modifiers,
                        leave: true,
                    },
                };
                self.push_input(input);
                self.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(runtime) = &self.runtime else { return };
        let now = self.started_at.elapsed();
        if runtime.has_pending_redraw()
            || runtime.next_timer_deadline().is_some_and(|deadline| deadline <= now)
            || !self.inputs.is_empty()
        {
            self.request_redraw();
            event_loop.set_control_flow(ControlFlow::Wait);
        } else if let Some(deadline) = runtime.next_timer_deadline() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.started_at + deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

fn logical_position(position: PhysicalPosition<f64>, scale_factor: f64) -> LogicalPoint {
    LogicalPoint { x: (position.x / scale_factor) as f32, y: (position.y / scale_factor) as f32 }
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

    fn screen(&mut self) -> PhysicalRect { self.renderer.screen() }

    fn scale_factor(&mut self) -> f32 { self.renderer.scale_factor() }

    fn repaint_buffer(&self) -> RepaintBuffer { RepaintBuffer::Swapped }

    fn create_image(&mut self, data: ImageData) -> ImageId { self.renderer.create_image(data) }

    fn drop_image(&mut self, image: ImageId) { self.renderer.drop_image(image) }

    fn create_string(&mut self, string: StringData) -> StringId { self.renderer.create_string(string) }

    fn drop_string(&mut self, string: StringId) { self.renderer.drop_string(string) }

    fn string(&self, string: StringId) -> &str { self.renderer.string(string) }

    fn text_offset_at_position(&mut self, request: &TextRequest, position: LogicalPoint) -> usize {
        self.renderer.text_offset_at_position(request, position)
    }

    fn measure_text(&mut self, request: &TextRequest) -> LogicalSize { self.renderer.measure_text(request) }

    fn measure_text_height(&mut self, request: &TextRequest) -> f32 {
        self.renderer.measure_text_height(request)
    }

    fn text_cursor_rect(&mut self, request: &TextRequest, byte_offset: usize) -> LogicalRect {
        self.renderer.text_cursor_rect(request, byte_offset)
    }

    fn show_keyboard(&mut self, _: &KeyboardRequest<'_>) { self.ime_requested = true }
}
