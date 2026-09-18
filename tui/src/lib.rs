//! a cell based terminal platform for blit
//!
//! frames are painted into a terminal cell grid. the renderer compares each
//! frame with the previous one and emits escape sequences only for changed
//! cells and Kitty graphics.
//!
//! [`Session`] owns `/dev/tty`, raw mode, keyboard and mouse input, resize
//! handling, frame timing, and presentation. call [`Session::pump`] when the
//! application owns its loop, or use [`run`] to drive a render closure.
//!
//! the built in runner targets Unix terminals with the kitty keyboard protocol,
//! such as Kitty and Ghostty. Windows and legacy terminals are not supported.

mod platform;
mod protocol;
mod renderer;
mod terminal;

pub mod atom;
pub mod widget;
pub use blit_layout as layout;
pub use platform::{BoundsClip, TuiPlatform};
pub use renderer::{RendererConfig, TuiRenderer, cell, color, image, text};

pub type Ui<'a, S = blit::state::Build> = blit::Ui<'a, TuiPlatform, S>;

use std::{
    io, io::Write as _, os::unix::net::UnixStream, sync::Arc, time::Duration, time::Instant,
};

use blit::{
    Frame, FrameInfo, LayoutResolution, LogicalPoint, LogicalSize,
    input::{Input, Key, KeyInput, Modifiers, PointerButton, ScrollPhase},
};
use terminal::{Size, Terminal};

const MAX_EVENTS_PER_FRAME: usize = 32;

/// runs the ui with the built in terminal and event loop at up to 240 frames per second
pub fn run(render: impl FnMut(Ui<'_>)) -> io::Result<()> {
    let mut session = Session::new()?;
    let mut render = render;
    while session.pump(&mut render)? {}
    Ok(())
}

/// owns terminal setup, input, presentation and frame timing
///
/// call [`Session::pump`] to drive it from an application owned loop
pub struct Session {
    terminal: Terminal,
    platform: TuiPlatform,
    wake: WakeHandle,
    frame: Frame<TuiPlatform>,
    started: Instant,
    next_frame: Duration,
    frame_interval: Duration,
    active: bool,
    query_colors: bool,
}

impl Session {
    pub fn new() -> io::Result<Self> {
        let (wake, wake_writer) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        wake_writer.set_nonblocking(true)?;
        let terminal = Terminal::new(wake)?;
        let renderer = TuiRenderer::new(renderer_config(terminal.size()?)?);
        let platform = TuiPlatform::new(renderer);
        Ok(Self {
            terminal,
            platform,
            wake: WakeHandle {
                writer: Arc::new(wake_writer),
            },
            frame: Frame::default(),
            started: Instant::now(),
            next_frame: Duration::ZERO,
            frame_interval: Duration::from_nanos(4_166_667),
            active: true,
            query_colors: true,
        })
    }

    /// waits for and renders one event loop iteration
    /// returns false when quit
    pub fn pump(&mut self, mut render: impl FnMut(Ui<'_>)) -> io::Result<bool> {
        let mut inputs = [Input::None; MAX_EVENTS_PER_FRAME];
        let mut input_count = 0;
        let mut pending = self.frame.has_pending_redraw();
        let needs_frame =
            |poll: Poll| poll.input_count != 0 || poll.resized || poll.redraw || poll.woken;

        // buffer events until another frame is allowed
        loop {
            let now = self.started.elapsed();
            if now >= self.next_frame {
                break;
            }
            let remaining = self.next_frame - now;
            if input_count == inputs.len() {
                std::thread::sleep(remaining);
                break;
            }
            let events = self.poll(Some(remaining), &mut inputs[input_count..])?;
            input_count += events.input_count;
            pending |= needs_frame(events);
        }

        // once allowed, wait for a reason to render
        while !pending {
            let now = self.started.elapsed();
            let timer = self.frame.next_timer_deadline();
            if timer.is_some_and(|deadline| deadline <= now) {
                break;
            }
            let events = self.poll(
                timer.map(|deadline| deadline - now),
                &mut inputs[input_count..],
            )?;
            input_count += events.input_count;
            pending |= needs_frame(events);
        }

        // replay the collected inputs and draw the final resulting tree
        let now = self.started.elapsed();
        let info = self.frame_info();
        self.frame.render_inputs(
            &mut self.platform,
            info,
            now,
            inputs[..input_count].iter().copied(),
            |mut ui| {
                if !ui.platform().should_quit() {
                    render(ui);
                }
            },
        );
        if self.platform().should_quit() {
            self.finish()?;
            return Ok(false);
        }
        self.present()?;
        // establish the earliest time for the following frame
        self.next_frame = now + self.frame_interval;
        Ok(true)
    }

    /// sets the minimum interval between rendered frames
    ///
    /// calculate it with `Duration::from_secs_f64(1.0 / frames_per_second)`
    pub fn set_frame_interval(&mut self, interval: Duration) {
        self.frame_interval = interval;
    }

    /// returns a handle that interrupts a pending [`Session::poll`]
    pub fn wake_handle(&self) -> WakeHandle {
        self.wake.clone()
    }

    pub fn platform(&self) -> &TuiPlatform {
        &self.platform
    }

    pub fn platform_mut(&mut self) -> &mut TuiPlatform {
        &mut self.platform
    }

    pub fn frame_info(&self) -> FrameInfo {
        let screen = self.platform.renderer().screen();
        FrameInfo::new(LogicalSize::new(screen.width as f32, screen.height as f32))
            .layout_resolution(LayoutResolution::Discrete {
                step: LogicalSize::uniform(1.0),
            })
    }

    pub fn poll(&mut self, timeout: Option<Duration>, inputs: &mut [Input]) -> io::Result<Poll> {
        if inputs.is_empty() {
            return Ok(Poll::default());
        }
        self.update_colors()?;
        let mut result = Poll::default();
        let mut timeout = timeout;
        for _ in 0..inputs.len() {
            let Some(event) = self.terminal.read(timeout)? else {
                break;
            };
            timeout = Some(Duration::ZERO);
            let event = match event {
                terminal::Event::Wake => {
                    result.woken = true;
                    break;
                }
                terminal::Event::Resize(size) => {
                    self.platform.renderer_mut().resize(renderer_config(size)?);
                    result.resized = true;
                    break;
                }
                terminal::Event::Protocol(event) => event,
            };
            match &event {
                protocol::Event::Focus(true) | protocol::Event::Theme(_) => {
                    self.query_colors = true;
                }
                protocol::Event::Color { slot, rgb } => {
                    result.redraw |= self.platform.renderer_mut().set_palette_color(*slot, *rgb);
                }
                _ => {}
            }
            let input = match event {
                protocol::Event::Text(character) => Some(Input::Text(character)),
                protocol::Event::Key(key) => {
                    let modifiers = Modifiers::new(
                        key.modifiers.shift,
                        key.modifiers.control,
                        key.modifiers.alt,
                        key.modifiers.super_key,
                    );
                    let logical = match key.code {
                        protocol::KeyCode::Character(character) => {
                            Some(Key::Character(if modifiers.shift() {
                                key.shifted.unwrap_or(character)
                            } else {
                                character
                            }))
                        }
                        protocol::KeyCode::Escape => Some(Key::Escape),
                        protocol::KeyCode::Enter => Some(Key::Enter),
                        protocol::KeyCode::Tab => Some(Key::Tab),
                        protocol::KeyCode::Backspace => Some(Key::Backspace),
                        protocol::KeyCode::Insert => Some(Key::Insert),
                        protocol::KeyCode::Delete => Some(Key::Delete),
                        protocol::KeyCode::Left => Some(Key::ArrowLeft),
                        protocol::KeyCode::Right => Some(Key::ArrowRight),
                        protocol::KeyCode::Up => Some(Key::ArrowUp),
                        protocol::KeyCode::Down => Some(Key::ArrowDown),
                        protocol::KeyCode::PageUp => Some(Key::PageUp),
                        protocol::KeyCode::PageDown => Some(Key::PageDown),
                        protocol::KeyCode::Home => Some(Key::Home),
                        protocol::KeyCode::End => Some(Key::End),
                        protocol::KeyCode::Function(number) => Some(Key::Function(number)),
                        protocol::KeyCode::Unknown => None,
                    };
                    match logical {
                        Some(Key::Character(character))
                            if !modifiers.control()
                                && !modifiers.alt()
                                && !modifiers.super_key() =>
                        {
                            (!key.text && key.kind != protocol::KeyKind::Release)
                                .then_some(Input::Text(character))
                        }
                        Some(key_code) => Some(Input::Key(KeyInput {
                            key: key_code,
                            modifiers,
                            pressed: key.kind != protocol::KeyKind::Release,
                            repeat: key.kind == protocol::KeyKind::Repeat,
                        })),
                        None => None,
                    }
                }
                protocol::Event::Mouse {
                    kind,
                    modifiers,
                    column,
                    row,
                } => {
                    let position = LogicalPoint {
                        x: f32::from(column) + 0.5,
                        y: f32::from(row) + 0.5,
                    };
                    let modifiers = Modifiers::new(
                        modifiers.shift,
                        modifiers.control,
                        modifiers.alt,
                        modifiers.super_key,
                    );
                    let button = |button| match button {
                        protocol::MouseButton::Left => PointerButton::Primary,
                        protocol::MouseButton::Middle => PointerButton::Middle,
                        protocol::MouseButton::Right => PointerButton::Secondary,
                        protocol::MouseButton::Back => PointerButton::Back,
                        protocol::MouseButton::Forward => PointerButton::Forward,
                        protocol::MouseButton::Other(number) => PointerButton::Other(number),
                    };
                    Some(match kind {
                        protocol::MouseKind::Down(value) => Input::PointerDown {
                            position,
                            button: button(value),
                            modifiers,
                        },
                        protocol::MouseKind::Up(value) => Input::PointerUp {
                            position,
                            button: button(value),
                            modifiers,
                            leave: false,
                        },
                        protocol::MouseKind::Move => Input::PointerMove {
                            position,
                            modifiers,
                        },
                        protocol::MouseKind::Scroll { x, y } => Input::Scroll {
                            position,
                            delta_x: f32::from(x) * 3.0,
                            delta_y: f32::from(y) * 3.0,
                            modifiers,
                            continuous: false,
                            phase: ScrollPhase::Moved,
                        },
                    })
                }
                _ => None,
            };
            if let Some(input) = input {
                if result.input_count != 0
                    && matches!(input, Input::PointerMove { .. })
                    && matches!(inputs[result.input_count - 1], Input::PointerMove { .. })
                {
                    inputs[result.input_count - 1] = input;
                } else {
                    inputs[result.input_count] = input;
                    result.input_count += 1;
                }
            }
        }
        self.update_colors()?;
        Ok(result)
    }

    pub fn present(&mut self) -> io::Result<()> {
        let output = self.platform.renderer().output();
        // avoid flushing when rendering produced no terminal changes
        if output.is_empty() {
            return Ok(());
        }
        self.terminal.write_all(output)?;
        self.terminal.flush()
    }

    pub fn finish(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let clear = self
            .platform
            .renderer_mut()
            .clear_kitty_graphics(&mut self.terminal);
        let finish = self.terminal.finish();
        clear.and(finish)
    }

    fn update_colors(&mut self) -> io::Result<()> {
        if self.query_colors {
            protocol::query_colors(&mut self.terminal)?;
            self.terminal.flush()?;
            self.query_colors = false;
        }
        Ok(())
    }
}

/// interrupts a [`Session`] waiting for terminal events
#[derive(Clone)]
pub struct WakeHandle {
    writer: Arc<UnixStream>,
}

impl WakeHandle {
    /// interrupts the session poll, coalescing with any pending wake
    pub fn wake(&self) {
        loop {
            match (&*self.writer).write(&[1]) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                _ => return,
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Poll {
    pub input_count: usize,
    pub resized: bool,
    /// terminal colors changed and the frame needs rebuilding
    pub redraw: bool,
    /// a [`WakeHandle`] interrupted the poll
    pub woken: bool,
}

fn renderer_config(size: Size) -> io::Result<RendererConfig> {
    if size.cols == 0 || size.rows == 0 {
        return Err(io::Error::other("terminal reported an empty window"));
    }
    Ok(RendererConfig::new().columns(size.cols).rows(size.rows))
}
