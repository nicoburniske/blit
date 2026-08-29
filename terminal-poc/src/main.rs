use std::{
    fs::File,
    io::{self, stdout},
    time::Duration,
};

use blit::{
    Ui, UiState,
    color::Color,
    container::Sizing,
    geometry::Sides,
    layout::{Align, Flex},
    repaint::FullRepaint,
    style::Style,
    text::TextWrap,
    widget::Text,
};
use blit_terminal_poc::TerminalRenderer;
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
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

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("--snapshot") => {
            let mut renderer = TerminalRenderer::new(96, 32);
            render_frame(&mut renderer, 1);
            print!("{}", renderer.plain_text());
            Ok(())
        }
        Some("--svg") => {
            let path = args
                .next()
                .unwrap_or_else(|| "terminal-poc/demo.svg".into());
            let mut renderer = TerminalRenderer::new(96, 32);
            render_frame(&mut renderer, 1);
            renderer.write_svg(File::create(path)?)
        }
        _ => run_interactive(),
    }
}

fn run_interactive() -> io::Result<()> {
    let (columns, rows) = size()?;
    let mut renderer = TerminalRenderer::new(columns, rows);
    let mut state = UiState::new(renderer.screen(), 1.0);
    let mut selected = 1;
    let mut output = stdout();
    enable_raw_mode()?;
    execute!(output, EnterAlternateScreen, Hide)?;
    let result = (|| -> io::Result<()> {
        loop {
            render_with_state(&mut renderer, &mut state, selected);
            renderer.present(&mut output)?;
            if !event::poll(Duration::from_millis(100))? {
                continue;
            }
            match event::read()? {
                Event::Resize(columns, rows) => {
                    renderer.resize(columns, rows);
                    state.set_screen(renderer.screen());
                }
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Left | KeyCode::Up => selected = selected.saturating_sub(1),
                    KeyCode::Right | KeyCode::Down => selected = (selected + 1).min(2),
                    _ => {}
                },
                _ => {}
            }
        }
        Ok(())
    })();
    let restore = execute!(output, Show, LeaveAlternateScreen).and_then(|_| disable_raw_mode());
    result.and(restore)
}

fn render_frame(renderer: &mut TerminalRenderer, selected: usize) {
    let mut state = UiState::new(renderer.screen(), 1.0);
    render_with_state(renderer, &mut state, selected);
}

fn render_with_state(renderer: &mut TerminalRenderer, state: &mut UiState, selected: usize) {
    blit::render(
        renderer,
        state,
        &mut FullRepaint,
        Duration::ZERO,
        [],
        |ui| {
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
                    let mut badge = ui
                        .layout(Flex::row().padding(Sides::x(8.0)))
                        .height(Sizing::fixed(32.0))
                        .style(Style::new().background(ACCENT).uniform_radius(8.0))
                        .open();
                    badge.add(Text::new("● LIVE").color(BACKGROUND).text_weight(700));
                });
            });
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
                .style(Style::new().background(SURFACE).solid_border(4.0, SURFACE_HIGH))
                .open();
            sidebar.add(Text::new("WORKSPACE").color(MUTED).text_weight(700));
            for (index, label) in ["Overview", "Text APIs", "Pixel map"].iter().enumerate() {
                sidebar.add(|ui: &mut Ui| {
                    let active = index == selected;
                    let mut item = ui
                        .layout(Flex::row().padding(Sides::x(8.0)))
                        .height(Sizing::fixed(32.0))
                        .style(Style::new().background(if active { SURFACE_HIGH } else { SURFACE }))
                        .open();
                    item.add(Text::new(if active { "› " } else { "  " }).color(ACCENT));
                    item.add(Text::new(label).color(if active { TEXT } else { MUTED }));
                });
            }
            sidebar.add(
                Text::new("↑ ↓  switch\nq     quit")
                    .color(MUTED)
                    .slot(blit::container::Slot::new().height(Sizing::grow()))
                    .vertical_align(blit::text::VerticalAlign::Bottom),
            );
        });
        body.add(|ui: &mut Ui| {
            let mut content = ui.layout(Flex::column().gap(16.0)).grow().open();
            content.add(|ui: &mut Ui| {
                let mut hero = ui
                    .layout(Flex::column().padding(Sides::all(16.0)).gap(8.0))
                    .width(Sizing::grow())
                    .height(Sizing::fixed(80.0))
                    .style(Style::new().background(SURFACE).solid_border(4.0, ACCENT))
                    .open();
                hero.add(Text::new("A framebuffer with a terminal on the other end.").color(TEXT).text_weight(700));
                hero.add(
                    Text::new("Blit flex layout and Text widgets produced this frame. Rectangles are sampled into Unicode quadrants; graphemes stay native.")
                        .color(MUTED)
                        .wrap(TextWrap::Word),
                );
            });
            content.add(|ui: &mut Ui| {
                let mut cards = ui
                    .layout(Flex::row().gap(16.0))
                    .width(Sizing::grow())
                    .height(Sizing::fixed(80.0))
                    .open();
                for (value, label, color) in [
                    ("2×2", "subpixels / cell", ACCENT),
                    ("24-bit", "ANSI color", BLUE),
                    ("100%", "Blit layout", PINK),
                ] {
                    cards.add(|ui: &mut Ui| {
                        let mut card = ui
                            .layout(Flex::column().padding(Sides::all(16.0)))
                            .grow()
                            .style(Style::new().background(SURFACE_HIGH))
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
                    .style(Style::new().background(SURFACE))
                    .open();
                panel.add(Text::new("PORTABILITY NOTES").color(MUTED).text_weight(700));
                panel.add(
                    Text::new("✓ word/character wrap • horizontal/vertical align").color(TEXT),
                );
                panel.add(Text::new("✓ clipping, opacity, borders, images").color(TEXT));
                panel.add(
                    Text::new("✓ Unicode graphemes and wide-cell measurement").color(TEXT),
                );
                panel.add(
                    Text::new("△ radius, gradients, shadows are approximations").color(PINK),
                );
            });
        });
            });
        },
    );
}
