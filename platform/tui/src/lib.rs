//! build terminal interfaces with blit's layouts, widgets and drawing atoms
//!
//! start with [`run`] and a closure that builds your ui using [`layout`],
//! [`widget`] and [`atom`]. the runner handles terminal setup, input and drawing.
//!
//! to own the event loop, use [`Session`] for terminal handling. to bring your
//! own backend, use [`TuiPlatform`] with [`blit::Frame`] and write the renderer's
//! output yourself.
//!
//! the built in runner targets Unix terminals with the kitty keyboard protocol,
//! such as Kitty and Ghostty. Windows and legacy terminals are not supported.

mod platform;
mod protocol;
mod terminal;

pub mod atom;
pub mod widget;
pub use blit_std::layout;
pub use blit_tui_render::{RendererConfig, TuiRenderer, cell, color, image, text};
pub use platform::{BoundsClip, TuiPlatform};

pub type Ui<'a, S = blit::state::Build> = blit::Ui<'a, TuiPlatform, S>;

use std::{io, io::Write as _, time::Duration, time::Instant};

use blit::{
    Frame, FrameInfo, LayoutResolution, LogicalPoint, LogicalSize,
    input::{Input, Key, KeyInput, Modifiers, PointerButton, ScrollPhase},
};
use terminal::{Size, Terminal};

const MAX_EVENTS_PER_FRAME: usize = 32;
const FRAME_INTERVAL: Duration = Duration::from_nanos(4_166_667);

/// runs the ui with the built in terminal and event loop at up to 240 frames per second
pub fn run(mut render: impl FnMut(Ui<'_>)) -> io::Result<()> {
    run_with(|_| (), move |_, ui| render(ui))
}

/// initializes application state before entering the built in event loop
pub fn run_with<S>(
    initialize: impl FnOnce(&mut TuiPlatform) -> S,
    mut render: impl FnMut(&mut S, Ui<'_>),
) -> io::Result<()> {
    let mut session = Session::new()?;
    let mut state = initialize(session.platform_mut());
    let mut frame = Frame::default();
    let result = (|| -> io::Result<()> {
        let start = Instant::now();
        let mut now = Duration::ZERO;
        let mut inputs = [Input::None; MAX_EVENTS_PER_FRAME];
        let mut input_count = 0;
        loop {
            let info = session.frame_info();
            frame.render_inputs(
                session.platform_mut(),
                info,
                now,
                inputs[..input_count].iter().copied(),
                |mut ui| {
                    if !ui.platform().should_quit() {
                        render(&mut state, ui);
                    }
                },
            );
            if session.platform().should_quit() {
                break;
            }
            session.present()?;
            let next_frame = now + FRAME_INTERVAL;
            input_count = 0;
            let mut redraw = frame.has_pending_redraw();
            loop {
                now = start.elapsed();
                let deadline = if redraw || input_count != 0 {
                    Some(next_frame)
                } else {
                    frame
                        .next_timer_deadline()
                        .map(|deadline| deadline.max(next_frame))
                };
                if input_count == inputs.len() {
                    std::thread::sleep(next_frame.saturating_sub(now));
                    now = start.elapsed();
                    break;
                }
                let poll = session.poll(
                    deadline.map(|deadline| deadline.saturating_sub(now)),
                    &mut inputs[input_count..],
                )?;
                input_count += poll.input_count;
                if poll.resized {
                    session.platform_mut().renderer_mut().invalidate();
                }
                redraw |= poll.resized || poll.redraw;
                now = start.elapsed();
                let timer_due = frame
                    .next_timer_deadline()
                    .is_some_and(|deadline| deadline <= now);
                if (redraw || input_count != 0 || timer_due) && now >= next_frame {
                    break;
                }
            }
        }
        Ok(())
    })();
    let finish = session.finish();
    result.and(finish)
}

/// terminal setup, input and presentation for an application owned event loop
///
/// use [`TuiPlatform`] directly when supplying your own terminal backend
pub struct Session {
    terminal: Terminal,
    platform: TuiPlatform,
    active: bool,
    query_colors: bool,
}

impl Session {
    pub fn new() -> io::Result<Self> {
        let terminal = Terminal::new()?;
        let renderer = TuiRenderer::new(renderer_config(terminal.size()?)?);
        let platform = TuiPlatform::new(renderer);
        Ok(Self {
            terminal,
            platform,
            active: true,
            query_colors: true,
        })
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
}

fn renderer_config(size: Size) -> io::Result<RendererConfig> {
    if size.cols == 0 || size.rows == 0 {
        return Err(io::Error::other("terminal reported an empty window"));
    }
    Ok(RendererConfig::new().columns(size.cols).rows(size.rows))
}
