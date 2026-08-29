use std::{io, io::Write as _, time::Duration, time::Instant};

use blit::{
    Ui, UiState,
    geometry::{LogicalPoint, LogicalSize, Scale2},
    input::{Input, Key, KeyInput, Modifiers, PointerButton, ScrollPhase},
    renderer::Renderer as _,
    repaint::{IncrementalRepaint, MyersTracker},
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

pub fn run(mut render: impl FnMut(&mut Ui) -> ControlFlow) -> io::Result<()> {
    let mut terminal = PlatformTerminal::new()?;
    let size = terminal.get_dimensions()?;
    let mut renderer = TerminalRenderer::new(renderer_config(size)?);
    let mut state = UiState::default();
    let mut repaint = IncrementalRepaint::new(MyersTracker::default(), false);
    terminal.enter_raw_mode()?;
    write!(
        terminal,
        "\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1015h\x1b[?1006h"
    )?;
    terminal.flush()?;
    let result = (|| -> io::Result<()> {
        let start = Instant::now();
        let mut control = ControlFlow::Continue;
        blit::render(
            &mut renderer,
            &mut state,
            &mut repaint,
            Duration::ZERO,
            [],
            |ui| {
                if render(ui) == ControlFlow::Exit {
                    control = ControlFlow::Exit;
                }
            },
        );
        terminal.write_all(renderer.output())?;
        terminal.flush()?;
        while control == ControlFlow::Continue {
            let now = start.elapsed();
            let timeout = if state.has_pending_redraw() {
                Some(Duration::ZERO)
            } else {
                state
                    .next_timer_deadline()
                    .map(|deadline| deadline.saturating_sub(now))
            };
            let mut inputs = [Input::None; MAX_EVENTS_PER_FRAME];
            let scale = renderer.geometry().physical_per_logical;
            let (input_count, resized) = poll_inputs(&terminal, timeout, scale, &mut inputs)?;
            let resized = if let Some(size) = resized {
                renderer.resize(renderer_config(size)?);
                true
            } else {
                false
            };
            let now = start.elapsed();
            let timer_due = state
                .next_timer_deadline()
                .is_some_and(|deadline| deadline <= now);
            if resized || input_count != 0 || state.has_pending_redraw() || timer_due {
                blit::render(
                    &mut renderer,
                    &mut state,
                    &mut repaint,
                    now,
                    inputs[..input_count].iter().copied(),
                    |ui| {
                        if render(ui) == ControlFlow::Exit {
                            control = ControlFlow::Exit;
                        }
                    },
                );
                terminal.write_all(renderer.output())?;
                terminal.flush()?;
            }
        }
        Ok(())
    })();
    let restore = renderer
        .clear_kitty_graphics(&mut terminal)
        .and_then(|_| {
            write!(
                terminal,
                "\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l\x1b[?25h\x1b[?1049l"
            )?;
            terminal.flush()
        })
        .and_then(|_| terminal.enter_cooked_mode());
    result.and(restore)
}

fn renderer_config(size: WindowSize) -> io::Result<RendererConfig> {
    let Some(pixel_width) = size.pixel_width.filter(|width| *width != 0) else {
        return Err(io::Error::other("terminal did not report its pixel width"));
    };
    let Some(pixel_height) = size.pixel_height.filter(|height| *height != 0) else {
        return Err(io::Error::other("terminal did not report its pixel height"));
    };
    if size.cols == 0 || size.rows == 0 {
        return Err(io::Error::other("terminal reported an empty window"));
    }
    Ok(RendererConfig::new()
        .columns(size.cols)
        .rows(size.rows)
        .cell_size(LogicalSize {
            width: f32::from(pixel_width) / f32::from(size.cols),
            height: f32::from(pixel_height) / f32::from(size.rows),
        }))
}

fn poll_inputs(
    terminal: &PlatformTerminal,
    timeout: Option<Duration>,
    scale: Scale2,
    inputs: &mut [Input],
) -> io::Result<(usize, Option<WindowSize>)> {
    if inputs.is_empty() || !terminal.poll(|_| true, timeout)? {
        return Ok((0, None));
    }

    let mut input_count = 0;
    let mut event_count = 0;
    loop {
        let terminal_event = terminal.read(|_| true)?;
        event_count += 1;
        let input = match terminal_event {
            TerminalEvent::WindowResized(size) => return Ok((0, Some(size))),
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
        if event_count == inputs.len() || !terminal.poll(|_| true, Some(Duration::ZERO))? {
            break;
        }
    }
    Ok((input_count, None))
}
