use std::{sync::Arc, time::Instant};

use blit::{
    Frame, FrameInfo, LogicalPoint, Size,
    input::{Input, Key, KeyInput, Modifiers, PointerButton, ScrollPhase},
};
use blit_executor::LocalExecutor;
use blit_gui::{GuiContext, TextLayoutEngine, TextSystem, Ui};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize as WindowSize, PhysicalPosition, PhysicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key as WindowKey, NamedKey},
    window::{Window, WindowId},
};

use crate::{Application, Config, EventLoopProxy, GraphicsBackend, RunError};

pub enum Event<T> {
    Input(T),
    TasksReady,
}

pub fn run<A>(config: Config<impl TextLayoutEngine>) -> Result<(), RunError>
where
    A: Application,
{
    let Config {
        title,
        width,
        height,
        text_config,
        text,
        graphics,
    } = config;
    let gui = GuiContext::new(TextSystem::new(text_config, text)?);
    let event_loop = EventLoop::<Event<A::Input>>::with_user_event().build()?;
    let mut runner: Runner<A> = Runner {
        state: Some(State::Pending(Pending {
            title,
            width,
            height,
            gui,
            graphics,
            input: EventLoopProxy {
                inner: event_loop.create_proxy(),
            },
        })),
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
    inputs: Vec<Input>,
    cursor: Option<PhysicalPosition<f64>>,
    modifiers: Modifiers,
    started_at: Instant,
    error: Option<RunError>,
}

enum State<A: Application> {
    Pending(Pending<A>),
    Active(Box<Active<A>>),
}

struct Pending<A: Application> {
    title: String,
    width: u32,
    height: u32,
    gui: GuiContext,
    graphics: Box<dyn GraphicsBackend>,
    input: EventLoopProxy<A::Input>,
}

struct Active<A: Application> {
    app: A,
    executor: LocalExecutor<A>,
    gui: GuiContext,
    graphics: Box<dyn GraphicsBackend>,
    frame: Frame<GuiContext>,
    window: Arc<Window>,
    ui_scale: f32,
}

impl<A: Application> Active<A> {
    fn scale(&self) -> f32 {
        self.window.scale_factor() as f32 * self.ui_scale
    }

    fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), RunError> {
        self.graphics.resize(size)?;
        self.frame.request_frame();
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
            .frame
            .next_timer_deadline()
            .is_some_and(|deadline| time >= deadline);
        if self.inputs.is_empty() && !active.frame.has_pending_redraw() && !timer_due {
            return;
        }
        let scale = active.scale();
        let size = active.window.inner_size();
        let info = FrameInfo::new(Size::new(
            size.width as f32 / scale,
            size.height as f32 / scale,
        ));
        let no_input = self.inputs.is_empty().then_some(Input::None);
        for input in self.inputs.drain(..).chain(no_input) {
            active.gui.profiler_mut().begin_build();
            active
                .frame
                .build(&mut active.gui, info, time, input, |ui: Ui<'_>| {
                    active.app.render(ui)
                });
            active.gui.profiler_mut().begin_layout();
            active.frame.layout(&mut active.gui);
        }
        active.gui.profiler_mut().begin_paint();
        active.frame.paint(&mut active.gui);
        active.gui.profiler_mut().finish();
        let result = active.graphics.render(active.gui.render_input());
        active
            .gui
            .finish_frame(result.as_ref().copied().unwrap_or_default());
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }
}

impl<A: Application> ApplicationHandler<Event<A::Input>> for Runner<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Pending {
            title,
            width,
            height,
            mut gui,
            mut graphics,
            input,
        } = match self.state.take() {
            Some(State::Pending(pending)) => pending,
            Some(State::Active(mut active)) => {
                if let Err(error) = active.graphics.resume(active.window.clone()) {
                    return self.fail(event_loop, error);
                }
                let size = active.window.inner_size();
                if let Err(error) = active.resize(size) {
                    return self.fail(event_loop, error);
                }
                active.window.request_redraw();
                self.state = Some(State::Active(active));
                return;
            }
            None => return,
        };
        let attributes = Window::default_attributes()
            .with_title(title)
            .with_inner_size(WindowSize::new(width, height));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => return self.fail(event_loop, error),
        };
        let size = window.inner_size();
        let size = PhysicalSize::new(size.width.max(1), size.height.max(1));
        if let Err(error) = graphics.resume(window.clone()) {
            return self.fail(event_loop, error);
        }
        gui.set_scale(window.scale_factor() as f32);
        let frame = Frame::default();
        let wake = input.inner.clone();
        let executor = LocalExecutor::new(move || {
            let _ = wake.send_event(Event::TasksReady);
        });
        let root = executor.root();
        let app = A::new(input, root, &mut gui);
        let mut active = Box::new(Active {
            app,
            executor,
            gui,
            graphics,
            frame,
            window,
            ui_scale: 1.0,
        });
        if let Err(error) = active.resize(size) {
            return self.fail(event_loop, error);
        }
        active.window.request_redraw();
        self.state = Some(State::Active(active));
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        if let Some(State::Active(active)) = &mut self.state {
            active.graphics.suspend();
        }
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
            Event::TasksReady => active.executor.run_ready(&mut active.app),
        };
        if request_frame {
            active.frame.request_frame();
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
                if let Err(error) = active.resize(size) {
                    self.fail(event_loop, error);
                } else {
                    active.window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                active.gui.set_scale(scale_factor as f32 * active.ui_scale);
                if let Err(error) = active.resize(active.window.inner_size()) {
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
                active.window.request_redraw();
                self.cursor = Some(position);
                let position = logical_position(position, active.scale());
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
                    let position = logical_position(position, active.scale());
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
                let scale = active.scale();
                let position = self.cursor.map_or_else(
                    || {
                        let size = active.window.inner_size();
                        LogicalPoint {
                            x: size.width as f32 / scale / 2.0,
                            y: size.height as f32 / scale / 2.0,
                        }
                    },
                    |position| logical_position(position, scale),
                );
                let (delta_x, delta_y, continuous) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (-x * 40.0, -y * 40.0, false),
                    MouseScrollDelta::PixelDelta(delta) => (
                        (-delta.x / scale as f64) as f32,
                        (-delta.y / scale as f64) as f32,
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
                    if event.state == ElementState::Pressed && self.modifiers.control() {
                        let scale = match key {
                            Key::Character('+') | Key::Character('=') => {
                                Some(active.ui_scale + 0.25)
                            }
                            Key::Character('-') => Some(active.ui_scale - 0.25),
                            _ => None,
                        };
                        if let Some(scale) = scale {
                            active.ui_scale = scale.clamp(0.5, 4.0);
                            let scale = active.scale();
                            active.gui.set_scale(scale);
                        }
                    }
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
                let position = logical_position(touch.location, active.scale());
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
        if active.frame.has_pending_redraw()
            || active
                .frame
                .next_timer_deadline()
                .is_some_and(|deadline| deadline <= now)
            || !self.inputs.is_empty()
        {
            active.window.request_redraw();
            event_loop.set_control_flow(ControlFlow::Wait);
        } else if let Some(deadline) = active.frame.next_timer_deadline() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.started_at + deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

fn logical_position(position: PhysicalPosition<f64>, scale_factor: f32) -> LogicalPoint {
    LogicalPoint {
        x: (position.x / scale_factor as f64) as f32,
        y: (position.y / scale_factor as f64) as f32,
    }
}
