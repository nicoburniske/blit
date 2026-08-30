mod platform;

pub use blit_layout as layout;
pub use blit_term::{color, image, text};
pub use platform::{BoundsClip, TerminalPlatform, draw, widget};

use std::{io, io::Write as _, time::Duration, time::Instant};

use blit::{
    Frame, FrameInfo, LayoutResolution, LogicalPoint, LogicalSize, Ui,
    input::{Input, Key, KeyInput, Modifiers, PointerButton, ScrollPhase},
};
use blit_term::{RendererConfig, TerminalRenderer};
use termina::{
    Event as TerminalEvent, PlatformTerminal, Terminal, WindowSize,
    event::{
        KeyCode as TerminalKeyCode, KeyEventKind as TerminalKeyEventKind,
        Modifiers as TerminalModifiers, MouseButton as TerminalMouseButton, MouseEventKind,
    },
};

const MAX_EVENTS_PER_FRAME: usize = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlFlow {
    #[default]
    Continue,
    Exit,
}

pub fn run(mut render: impl FnMut(&mut Ui<'_, TerminalPlatform>) -> ControlFlow) -> io::Result<()> {
    run_with(|_| (), move |_, ui| render(ui))
}

pub fn run_with<S>(
    initialize: impl FnOnce(&mut TerminalPlatform) -> S,
    mut render: impl FnMut(&mut S, &mut Ui<'_, TerminalPlatform>) -> ControlFlow,
) -> io::Result<()> {
    let mut session = Session::new()?;
    let mut state = initialize(session.platform_mut());
    let mut frame = Frame::default();
    let result = (|| -> io::Result<()> {
        let start = Instant::now();
        let mut control = ControlFlow::Continue;
        let info = session.frame_info();
        frame.render_inputs(
            session.platform_mut(),
            info,
            Duration::ZERO,
            [],
            |mut ui| {
                if render(&mut state, &mut ui) == ControlFlow::Exit {
                    control = ControlFlow::Exit;
                }
            },
        );
        session.present()?;
        while control == ControlFlow::Continue {
            let now = start.elapsed();
            let timeout = if frame.has_pending_redraw() {
                Some(Duration::ZERO)
            } else {
                frame
                    .next_timer_deadline()
                    .map(|deadline| deadline.saturating_sub(now))
            };
            let mut inputs = [Input::None; MAX_EVENTS_PER_FRAME];
            let poll = session.poll(timeout, &mut inputs)?;
            if poll.resized {
                session.platform_mut().invalidate_all();
            }
            let now = start.elapsed();
            let timer_due = frame
                .next_timer_deadline()
                .is_some_and(|deadline| deadline <= now);
            if poll.resized || poll.input_count != 0 || frame.has_pending_redraw() || timer_due {
                let info = session.frame_info();
                frame.render_inputs(
                    session.platform_mut(),
                    info,
                    now,
                    inputs[..poll.input_count].iter().copied(),
                    |mut ui| {
                        if render(&mut state, &mut ui) == ControlFlow::Exit {
                            control = ControlFlow::Exit;
                        }
                    },
                );
                session.present()?;
            }
        }
        Ok(())
    })();
    let finish = session.finish();
    result.and(finish)
}

pub struct Session {
    terminal: PlatformTerminal,
    platform: TerminalPlatform,
    active: bool,
}

impl Session {
    const ENTER: &str =
        "\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1015h\x1b[?1006h";
    const LEAVE: &str =
        "\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l\x1b[?25h\x1b[?1049l";

    pub fn new() -> io::Result<Self> {
        let mut terminal = PlatformTerminal::new()?;
        let renderer = TerminalRenderer::new(renderer_config(terminal.get_dimensions()?)?);
        let platform = TerminalPlatform::new(renderer);
        terminal.enter_raw_mode()?;
        let mut session = Self {
            terminal,
            platform,
            active: true,
        };
        session.terminal.write_all(Self::ENTER.as_bytes())?;
        session.terminal.flush()?;
        Ok(session)
    }

    pub fn platform(&self) -> &TerminalPlatform {
        &self.platform
    }

    pub fn platform_mut(&mut self) -> &mut TerminalPlatform {
        &mut self.platform
    }

    pub fn frame_info(&self) -> FrameInfo {
        let renderer = self.platform.renderer();
        let scale = renderer.scale();
        let screen = renderer.screen().to_logical(scale);
        FrameInfo::new(screen.size()).layout_resolution(LayoutResolution::Discrete {
            step: renderer.cell_size(),
        })
    }

    pub fn poll(&mut self, timeout: Option<Duration>, inputs: &mut [Input]) -> io::Result<Poll> {
        if inputs.is_empty() || !self.terminal.poll(|_| true, timeout)? {
            return Ok(Poll::default());
        }

        let scale = self.platform.renderer().scale();
        let mut input_count = 0;
        let mut event_count = 0;
        loop {
            let terminal_event = self.terminal.read(|_| true)?;
            event_count += 1;
            let input = match terminal_event {
                TerminalEvent::WindowResized(size) => {
                    self.platform.renderer_mut().resize(renderer_config(size)?);
                    return Ok(Poll {
                        input_count,
                        resized: true,
                    });
                }
                TerminalEvent::Key(key) if key.kind == TerminalKeyEventKind::Press => {
                    let modifiers = Modifiers::new(
                        key.modifiers.contains(TerminalModifiers::SHIFT),
                        key.modifiers.contains(TerminalModifiers::CONTROL),
                        key.modifiers.contains(TerminalModifiers::ALT),
                        key.modifiers.contains(TerminalModifiers::SUPER),
                    );
                    let logical = match key.code {
                        TerminalKeyCode::Char(character)
                            if modifiers.control() || modifiers.alt() || modifiers.super_key() =>
                        {
                            Some(Key::Character(character))
                        }
                        TerminalKeyCode::Char(character) => {
                            inputs[input_count] = Input::Text(character);
                            input_count += 1;
                            None
                        }
                        TerminalKeyCode::Backspace => Some(Key::Backspace),
                        TerminalKeyCode::Delete => Some(Key::Delete),
                        TerminalKeyCode::Left => Some(Key::ArrowLeft),
                        TerminalKeyCode::Right => Some(Key::ArrowRight),
                        TerminalKeyCode::Up => Some(Key::ArrowUp),
                        TerminalKeyCode::Down => Some(Key::ArrowDown),
                        TerminalKeyCode::Enter => Some(Key::Enter),
                        TerminalKeyCode::Tab | TerminalKeyCode::BackTab => Some(Key::Tab),
                        TerminalKeyCode::Escape => Some(Key::Escape),
                        TerminalKeyCode::Home => Some(Key::Home),
                        TerminalKeyCode::End => Some(Key::End),
                        TerminalKeyCode::PageUp => Some(Key::PageUp),
                        TerminalKeyCode::PageDown => Some(Key::PageDown),
                        TerminalKeyCode::Insert => Some(Key::Insert),
                        TerminalKeyCode::Function(function) => Some(Key::Function(function)),
                        _ => None,
                    };
                    logical.map(|key| {
                        Input::Key(KeyInput {
                            key,
                            modifiers,
                            pressed: true,
                            repeat: false,
                        })
                    })
                }
                TerminalEvent::Mouse(mouse) => {
                    let position = LogicalPoint {
                        x: (f32::from(mouse.column) + 0.5) / scale.x,
                        y: (f32::from(mouse.row) + 0.5) / scale.y,
                    };
                    let modifiers = Modifiers::new(
                        mouse.modifiers.contains(TerminalModifiers::SHIFT),
                        mouse.modifiers.contains(TerminalModifiers::CONTROL),
                        mouse.modifiers.contains(TerminalModifiers::ALT),
                        mouse.modifiers.contains(TerminalModifiers::SUPER),
                    );
                    let button = match mouse.kind {
                        MouseEventKind::Down(button)
                        | MouseEventKind::Up(button)
                        | MouseEventKind::Drag(button) => match button {
                            TerminalMouseButton::Left => PointerButton::Primary,
                            TerminalMouseButton::Right => PointerButton::Secondary,
                            TerminalMouseButton::Middle => PointerButton::Middle,
                        },
                        _ => PointerButton::Primary,
                    };
                    Some(match mouse.kind {
                        MouseEventKind::Down(_) => Input::PointerDown {
                            position,
                            button,
                            modifiers,
                        },
                        MouseEventKind::Up(_) => Input::PointerUp {
                            position,
                            button,
                            modifiers,
                            leave: false,
                        },
                        MouseEventKind::Drag(_) | MouseEventKind::Moved => Input::PointerMove {
                            position,
                            modifiers,
                        },
                        MouseEventKind::ScrollUp
                        | MouseEventKind::ScrollDown
                        | MouseEventKind::ScrollLeft
                        | MouseEventKind::ScrollRight => Input::Scroll {
                            position,
                            delta_x: match mouse.kind {
                                MouseEventKind::ScrollLeft => -3.0 / scale.x,
                                MouseEventKind::ScrollRight => 3.0 / scale.x,
                                _ => 0.0,
                            },
                            delta_y: match mouse.kind {
                                MouseEventKind::ScrollUp => -3.0 / scale.y,
                                MouseEventKind::ScrollDown => 3.0 / scale.y,
                                _ => 0.0,
                            },
                            modifiers,
                            continuous: false,
                            phase: ScrollPhase::Moved,
                        },
                    })
                }
                _ => None,
            };
            if let Some(input) = input {
                if input_count != 0
                    && matches!(input, Input::PointerMove { .. })
                    && matches!(inputs[input_count - 1], Input::PointerMove { .. })
                {
                    inputs[input_count - 1] = input;
                } else {
                    inputs[input_count] = input;
                    input_count += 1;
                }
            }
            if event_count == inputs.len() || !self.terminal.poll(|_| true, Some(Duration::ZERO))? {
                break;
            }
        }
        Ok(Poll {
            input_count,
            resized: false,
        })
    }

    pub fn present(&mut self) -> io::Result<()> {
        self.terminal.write_all(self.platform.renderer().output())?;
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
        let leave = self.terminal.write_all(Self::LEAVE.as_bytes());
        let flush = self.terminal.flush();
        let cooked = self.terminal.enter_cooked_mode();
        clear.and(leave).and(flush).and(cooked)
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
}

fn renderer_config(size: WindowSize) -> io::Result<RendererConfig> {
    if size.cols == 0 || size.rows == 0 {
        return Err(io::Error::other("terminal reported an empty window"));
    }
    let mut config = RendererConfig::new().columns(size.cols).rows(size.rows);
    if let (Some(pixel_width), Some(pixel_height)) = (
        size.pixel_width.filter(|width| *width != 0),
        size.pixel_height.filter(|height| *height != 0),
    ) {
        config = config.cell_size(LogicalSize {
            width: f32::from(pixel_width) * f32::from(size.rows)
                / (f32::from(pixel_height) * f32::from(size.cols)),
            height: 1.0,
        });
    }
    Ok(config)
}
