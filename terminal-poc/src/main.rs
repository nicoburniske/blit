use std::{
    fs::File,
    io::{self, Write as _},
    time::{Duration, Instant},
};

use blit::{
    Ui, UiState,
    color::Color,
    container::{Sizing, Slot},
    geometry::{LogicalPoint, Sides},
    image::{ImageData, ImageFormat, ImageHandle, ImagePixels},
    input::{Input, Key, KeyInput, Modifiers, PointerButton, ScrollPhase},
    interact::{Sense, WidgetId},
    layout::{Align, Flex},
    renderer::Renderer as _,
    repaint::FullRepaint,
    style::Style,
    text::TextWrap,
    widget::{Image, Text, Widget},
};
use blit_terminal_poc::TerminalRenderer;
use termina::{
    Event as TerminalEvent, PlatformTerminal, Terminal,
    event::{
        KeyCode as TerminalKeyCode, KeyEventKind as TerminalKeyEventKind,
        Modifiers as TerminalModifiers, MouseButton as TerminalMouseButton, MouseEventKind,
    },
};

const BACKGROUND: Color = Color::from_rgba8(10, 15, 28, 255);
const SURFACE: Color = Color::from_rgba8(20, 29, 48, 255);
const SURFACE_HIGH: Color = Color::from_rgba8(29, 41, 65, 255);
const ACCENT: Color = Color::from_rgba8(86, 211, 194, 255);
const BLUE: Color = Color::from_rgba8(94, 159, 255, 255);
const PINK: Color = Color::from_rgba8(238, 119, 174, 255);
const TEXT: Color = Color::from_rgba8(231, 237, 248, 255);
const MUTED: Color = Color::from_rgba8(137, 151, 175, 255);
const MAX_EVENTS_PER_FRAME: usize = 32;

struct AppState {
    selected: usize,
    rounded: bool,
    image: ImageHandle,
}

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--snapshot") => {
            let mut renderer = TerminalRenderer::new(96, 32);
            render_frame(&mut renderer);
            print!("{}", renderer.plain_text());
            Ok(())
        }
        Some("--svg") => {
            let path = args
                .next()
                .unwrap_or_else(|| "terminal-poc/demo.svg".into());
            let mut renderer = TerminalRenderer::new(96, 32);
            render_frame(&mut renderer);
            renderer.write_svg(File::create(path)?)
        }
        _ => run_interactive(),
    }
}

fn run_interactive() -> io::Result<()> {
    let mut terminal = PlatformTerminal::new()?;
    let size = terminal.get_dimensions()?;
    let mut renderer = TerminalRenderer::new(size.cols, size.rows);
    let mut state = UiState::new(renderer.screen(), 1.0);
    let mut app = AppState {
        selected: 1,
        rounded: true,
        image: demo_image(&mut renderer),
    };
    terminal.enter_raw_mode()?;
    write!(
        terminal,
        "\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1015h\x1b[?1006h"
    )?;
    terminal.flush()?;
    let result = (|| -> io::Result<()> {
        let start = Instant::now();
        render_with_state(&mut renderer, &mut state, &mut app, Duration::ZERO, &[]);
        renderer.present(&mut terminal)?;
        'run: loop {
            let now = start.elapsed();
            let mut next_event = if state.has_pending_redraw() {
                if terminal.poll(|_| true, Some(Duration::ZERO))? {
                    Some(terminal.read(|_| true)?)
                } else {
                    None
                }
            } else if let Some(deadline) = state.next_timer_deadline() {
                if terminal.poll(|_| true, Some(deadline.saturating_sub(now)))? {
                    Some(terminal.read(|_| true)?)
                } else {
                    None
                }
            } else {
                Some(terminal.read(|_| true)?)
            };
            let mut inputs = [Input::None; MAX_EVENTS_PER_FRAME];
            let mut input_count = 0;
            let mut event_count = 0;
            while let Some(terminal_event) = next_event {
                event_count += 1;
                let input = match terminal_event {
                    TerminalEvent::WindowResized(size) => {
                        renderer.resize(size.cols, size.rows);
                        state.set_screen(renderer.screen());
                        None
                    }
                    TerminalEvent::Key(key) if key.kind == TerminalKeyEventKind::Press => match key
                        .code
                    {
                        TerminalKeyCode::Char('q') | TerminalKeyCode::Escape => break 'run,
                        TerminalKeyCode::Left => Some(Input::Key(KeyInput::new(Key::ArrowLeft))),
                        TerminalKeyCode::Right => Some(Input::Key(KeyInput::new(Key::ArrowRight))),
                        TerminalKeyCode::Up => Some(Input::Key(KeyInput::new(Key::ArrowUp))),
                        TerminalKeyCode::Down => Some(Input::Key(KeyInput::new(Key::ArrowDown))),
                        TerminalKeyCode::Char(character) => Some(Input::Text(character)),
                        _ => None,
                    },
                    TerminalEvent::Mouse(mouse) => {
                        let position = LogicalPoint {
                            x: (f32::from(mouse.column) + 0.5) * blit_terminal_poc::CELL_WIDTH,
                            y: (f32::from(mouse.row) + 0.5) * blit_terminal_poc::CELL_HEIGHT,
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
                                    MouseEventKind::ScrollLeft => {
                                        -blit_terminal_poc::CELL_WIDTH * 3.0
                                    }
                                    MouseEventKind::ScrollRight => {
                                        blit_terminal_poc::CELL_WIDTH * 3.0
                                    }
                                    _ => 0.0,
                                },
                                delta_y: match mouse.kind {
                                    MouseEventKind::ScrollUp => {
                                        -blit_terminal_poc::CELL_HEIGHT * 3.0
                                    }
                                    MouseEventKind::ScrollDown => {
                                        blit_terminal_poc::CELL_HEIGHT * 3.0
                                    }
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
                next_event = if event_count < MAX_EVENTS_PER_FRAME
                    && terminal.poll(|_| true, Some(Duration::ZERO))?
                {
                    Some(terminal.read(|_| true)?)
                } else {
                    None
                };
            }
            let now = start.elapsed();
            let timer_due = state
                .next_timer_deadline()
                .is_some_and(|deadline| deadline <= now);
            if input_count != 0 || state.has_pending_redraw() || timer_due {
                render_with_state(
                    &mut renderer,
                    &mut state,
                    &mut app,
                    now,
                    &inputs[..input_count],
                );
                renderer.present(&mut terminal)?;
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

fn render_frame(renderer: &mut TerminalRenderer) {
    let mut state = UiState::new(renderer.screen(), 1.0);
    let mut app = AppState {
        selected: 1,
        rounded: true,
        image: demo_image(renderer),
    };
    render_with_state(renderer, &mut state, &mut app, Duration::ZERO, &[]);
}

fn demo_image(renderer: &mut TerminalRenderer) -> ImageHandle {
    const WIDTH: usize = 96;
    const HEIGHT: usize = 64;
    let mut pixels = Vec::with_capacity(WIDTH * HEIGHT * 4);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let dx = x as f32 - WIDTH as f32 / 2.0;
            let dy = y as f32 - HEIGHT as f32 / 2.0;
            let inside = dx * dx / 1200.0 + dy * dy / 520.0 < 1.0;
            pixels.extend_from_slice(if inside {
                &[86, 211, 194, 255]
            } else if (x / 8 + y / 8) % 2 == 0 {
                &[29, 41, 65, 255]
            } else {
                &[20, 29, 48, 255]
            });
        }
    }
    renderer.create_image(ImageData::new(
        ImagePixels::Owned(pixels.into_boxed_slice()),
        ImageFormat::Rgba8,
        WIDTH,
        HEIGHT,
    ))
}

fn render_with_state(
    renderer: &mut TerminalRenderer,
    state: &mut UiState,
    app: &mut AppState,
    time: Duration,
    inputs: &[Input],
) {
    blit::render(
        renderer,
        state,
        &mut FullRepaint,
        time,
        inputs.iter().copied(),
        |ui| app_widget(app).render(ui),
    );
}

fn app_widget(app: &mut AppState) -> impl Widget<Output = ()> + '_ {
    |ui: &mut Ui| {
        if let Input::Key(key) = ui.input()
            && key.pressed
        {
            match key.key {
                Key::ArrowLeft | Key::ArrowUp => app.selected = app.selected.saturating_sub(1),
                Key::ArrowRight | Key::ArrowDown => app.selected = (app.selected + 1).min(2),
                _ => {}
            }
        }
        ui.clear();
        let mut root = ui
            .layout(Flex::column().padding(Sides::all(16.0)).gap(16.0))
            .grow()
            .style(Style::new().background(BACKGROUND))
            .open();
        root.add(|ui: &mut Ui| {
            let mut header = ui
                .layout(Flex::row().align(Align::Center).gap(16.0))
                .width(Sizing::grow())
                .height(Sizing::fixed(48.0))
                .open();
            header.add(Text::new("BLIT / TERMINAL").color(TEXT).text_weight(700));
            header.add(
                Text::new("same widgets • quantized backend")
                    .color(MUTED)
                    .slot(blit::container::Slot::new().width(Sizing::grow())),
            );
            header.add(|ui: &mut Ui| {
                let id = WidgetId::new("terminal corner toggle");
                let interaction = ui.interact(id, Sense::CLICK);
                if interaction.clicked {
                    app.rounded = !app.rounded;
                }
                let mut toggle = ui
                    .layout(Flex::row().padding(Sides::x(8.0)))
                    .id(id)
                    .height(Sizing::fixed(32.0))
                    .style(
                        Style::new()
                            .background(if interaction.hovered {
                                SURFACE_HIGH
                            } else {
                                SURFACE
                            })
                            .solid_border(4.0, ACCENT)
                            .uniform_radius(if app.rounded { 8.0 } else { 0.0 }),
                    )
                    .open();
                toggle.add(
                    Text::new(if app.rounded {
                        "CORNERS: ROUNDED"
                    } else {
                        "CORNERS: SQUARE"
                    })
                    .color(ACCENT)
                    .text_weight(700),
                );
            });
        });
        let radius = if app.rounded { 8.0 } else { 0.0 };
        root.add(|ui: &mut Ui| {
        let mut body = ui
            .layout(Flex::row().gap(16.0))
            .grow()
            .open();
        body.add(|ui: &mut Ui| {
            let mut sidebar = ui
                .layout(Flex::column().padding(Sides::all(16.0)).gap(16.0))
                .width(Sizing::fixed(168.0))
                .height(Sizing::grow())
                .style(
                    Style::new()
                        .background(SURFACE)
                        .solid_border(4.0, SURFACE_HIGH)
                        .uniform_radius(radius),
                )
                .open();
            sidebar.add(Text::new("WORKSPACE").color(MUTED).text_weight(700));
            for (index, label) in ["Overview", "Text APIs", "Pixel map"].iter().enumerate() {
                sidebar.add(|ui: &mut Ui| {
                    let id = WidgetId::new(("terminal navigation", index));
                    let interaction = ui.interact(id, Sense::CLICK);
                    if interaction.clicked {
                        app.selected = index;
                    }
                    let active = index == app.selected;
                    let mut item = ui
                        .layout(Flex::row().padding(Sides::x(8.0)))
                        .id(id)
                        .height(Sizing::fixed(32.0))
                        .style(Style::new().background(if active || interaction.hovered {
                            SURFACE_HIGH
                        } else {
                            SURFACE
                        }))
                        .open();
                    item.add(Text::new(if active { "› " } else { "  " }).color(ACCENT));
                    item.add(Text::new(label).color(if active { TEXT } else { MUTED }));
                });
            }
            sidebar.add(
                Text::new("mouse hover+click\n↑↓ select • q quit")
                    .color(MUTED)
                    .slot(blit::container::Slot::new().height(Sizing::grow()))
                    .vertical_align(blit::text::VerticalAlign::Bottom),
            );
        });
        body.add(|ui: &mut Ui| {
            let mut content = ui.layout(Flex::column().gap(16.0)).grow().open();
            content.add(|ui: &mut Ui| {
                let mut hero = ui
                    .layout(Flex::row().padding(Sides::all(16.0)).gap(16.0))
                    .width(Sizing::grow())
                    .height(Sizing::fixed(96.0))
                    .style(
                        Style::new()
                            .background(SURFACE)
                            .solid_border(4.0, ACCENT)
                            .uniform_radius(radius),
                    )
                    .open();
                hero.add(|ui: &mut Ui| {
                    let mut copy = ui
                        .layout(Flex::column().gap(8.0))
                        .width(Sizing::grow())
                        .open();
                    copy.add(
                        Text::new("Blit widgets, real terminal semantics.")
                            .color(TEXT)
                            .text_weight(700),
                    );
                    copy.add(
                        Text::new("Box-drawing borders, SGR mouse input, Blit click handlers, and a retained Kitty image placement.")
                            .color(MUTED)
                            .wrap(TextWrap::Word),
                    );
                });
                hero.add(
                    Image::new(&app.image).slot(
                        Slot::new()
                            .width(Sizing::fixed(96.0))
                            .height(Sizing::fixed(64.0)),
                    ),
                );
            });
            content.add(|ui: &mut Ui| {
                let mut cards = ui
                    .layout(Flex::row().gap(16.0))
                    .width(Sizing::grow())
                    .height(Sizing::fixed(80.0))
                    .open();
                for (value, label, color) in [
                    ("KITTY", "retained graphics", ACCENT),
                    ("SGR", "mouse events", BLUE),
                    ("BLIT", "real interactions", PINK),
                ] {
                    cards.add(|ui: &mut Ui| {
                        let mut card = ui
                            .layout(Flex::column().padding(Sides::all(16.0)))
                            .grow()
                            .style(
                                Style::new()
                                    .background(SURFACE_HIGH)
                                    .solid_border(4.0, color)
                                    .uniform_radius(radius),
                            )
                            .open();
                        card.add(Text::new(value).color(color).text_weight(700));
                        card.add(Text::new(label).color(MUTED));
                    });
                }
            });
            content.add(|ui: &mut Ui| {
                let mut panel = ui
                    .layout(Flex::column().padding(Sides::all(16.0)).gap(16.0))
                    .grow()
                    .style(
                        Style::new()
                            .background(SURFACE)
                            .solid_border(4.0, SURFACE_HIGH)
                            .uniform_radius(radius),
                    )
                    .open();
                panel.add(Text::new("PORTABILITY NOTES").color(MUTED).text_weight(700));
                panel.add(
                    Text::new("✓ word/character wrap • horizontal/vertical align").color(TEXT),
                );
                panel.add(Text::new("✓ box drawing with merged line joints").color(TEXT));
                panel.add(
                    Text::new("✓ Unicode graphemes and wide-cell measurement").color(TEXT),
                );
                panel.add(
                    Text::new("✓ Kitty RGBA transmission and retained placements").color(PINK),
                );
            });
        });
            });
    }
}
