use std::{hint::black_box, io};

use blit::{
    Ui, UiState,
    color::Color,
    command_list::{ClipId, CommandList, Rectangle},
    container::{Sizing, Slot},
    geometry::{LogicalPoint, LogicalRect, PhysicalRect, Sides},
    image::{ImageData, ImageFormat, ImageHandle, ImagePixels},
    input::{Input, Modifiers},
    interact::{Sense, WidgetId},
    layout::{Align, Flex},
    renderer::Renderer as _,
    repaint::{IncrementalRepaint, MyersTracker},
    style::Style,
    text::TextWrap,
    widget::{Image, Text},
};
use blit_terminal_poc::{CELL_HEIGHT, CELL_WIDTH, TerminalRenderer};
use divan::counter::ItemsCount;

const COLUMNS: u16 = 96;
const ROWS: u16 = 32;
const BACKGROUND: Color = Color::from_rgba8(10, 15, 28, 255);
const SURFACE: Color = Color::from_rgba8(20, 29, 48, 255);
const SURFACE_HIGH: Color = Color::from_rgba8(29, 41, 65, 255);
const ACCENT: Color = Color::from_rgba8(86, 211, 194, 255);
const TEXT: Color = Color::from_rgba8(231, 237, 248, 255);
const MUTED: Color = Color::from_rgba8(137, 151, 175, 255);

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

struct SceneState {
    selected: usize,
    expanded: bool,
}

#[derive(Clone, Copy, Debug)]
enum SceneUpdate {
    Stable,
    Hover,
    Local,
    Structural,
    Full,
}

fn main() {
    divan::main()
}

#[divan::bench]
fn render_full(bencher: divan::Bencher) {
    let commands = commands(false);
    let damage = [screen()];
    let mut renderer = TerminalRenderer::new(COLUMNS, ROWS);
    bencher
        .counter(ItemsCount::new(usize::from(COLUMNS) * usize::from(ROWS)))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

#[divan::bench]
fn render_one_cell(bencher: divan::Bencher) {
    let commands = commands(true);
    let damage = [PhysicalRect {
        x: 40 * CELL_WIDTH as i32,
        y: 12 * CELL_HEIGHT as i32,
        width: CELL_WIDTH as i32,
        height: CELL_HEIGHT as i32,
    }];
    let mut renderer = TerminalRenderer::new(COLUMNS, ROWS);
    renderer.render(&commands, &[screen()]);
    bencher
        .counter(ItemsCount::new(1usize))
        .bench_local(|| renderer.render(black_box(&commands), black_box(&damage)));
}

#[divan::bench]
fn present_unchanged(bencher: divan::Bencher) {
    let commands = commands(false);
    let mut renderer = TerminalRenderer::new(COLUMNS, ROWS);
    renderer.render(&commands, &[screen()]);
    renderer.present(&mut io::sink()).unwrap();
    bencher.bench_local(|| renderer.present(black_box(&mut io::sink())).unwrap());
}

#[divan::bench]
fn update_one_cell(bencher: divan::Bencher) {
    let old = commands(false);
    let new = commands(true);
    let damage = [PhysicalRect {
        x: 40 * CELL_WIDTH as i32,
        y: 12 * CELL_HEIGHT as i32,
        width: CELL_WIDTH as i32,
        height: CELL_HEIGHT as i32,
    }];
    let mut renderer = TerminalRenderer::new(COLUMNS, ROWS);
    renderer.render(&old, &[screen()]);
    renderer.present(&mut io::sink()).unwrap();
    let mut changed = true;
    bencher.bench_local(|| {
        renderer.render(
            black_box(if changed { &new } else { &old }),
            black_box(&damage),
        );
        renderer.present(black_box(&mut io::sink())).unwrap();
        changed = !changed;
    });
}

#[divan::bench(args = [
    SceneUpdate::Stable,
    SceneUpdate::Hover,
    SceneUpdate::Local,
    SceneUpdate::Structural,
    SceneUpdate::Full,
])]
fn representative_ui(bencher: divan::Bencher, update: SceneUpdate) {
    let mut renderer = TerminalRenderer::new(COLUMNS, ROWS);
    let image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&[86, 211, 194, 255]),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    let mut state = UiState::new(renderer.screen(), 1.0);
    let mut repaint = IncrementalRepaint::new(MyersTracker::default(), false);
    let mut scene = SceneState {
        selected: 1,
        expanded: false,
    };
    blit::render(
        &mut renderer,
        &mut state,
        &mut repaint,
        std::time::Duration::ZERO,
        [],
        |ui| representative_scene(ui, &scene, &image),
    );
    renderer.present(&mut io::sink()).unwrap();

    let mut alternate = false;
    bencher.bench_local(|| {
        let input = match update {
            SceneUpdate::Stable => Input::None,
            SceneUpdate::Hover => Input::PointerMove {
                position: LogicalPoint {
                    x: 80.0,
                    y: if alternate { 124.0 } else { 156.0 },
                },
                modifiers: Modifiers::NONE,
            },
            SceneUpdate::Local => {
                scene.selected = usize::from(alternate);
                Input::None
            }
            SceneUpdate::Structural => {
                scene.expanded = alternate;
                Input::None
            }
            SceneUpdate::Full => {
                scene.selected = usize::from(alternate);
                state.invalidate_all();
                Input::None
            }
        };
        blit::render(
            black_box(&mut renderer),
            black_box(&mut state),
            black_box(&mut repaint),
            std::time::Duration::ZERO,
            [black_box(input)],
            |ui| representative_scene(ui, &scene, &image),
        );
        renderer.present(black_box(&mut io::sink())).unwrap();
        alternate = !alternate;
    });
}

fn representative_scene(ui: &mut Ui, state: &SceneState, image: &ImageHandle) {
    ui.clear();
    let mut root = ui
        .layout(Flex::column().padding(Sides::all(16.0)).gap(12.0))
        .grow()
        .style(Style::new().background(BACKGROUND))
        .open();
    root.add(|ui: &mut Ui| {
        let mut header = ui
            .layout(Flex::row().align(Align::Center).gap(12.0))
            .width(Sizing::grow())
            .height(Sizing::fixed(48.0))
            .open();
        header.add(Text::new("BLIT OPERATIONS").color(TEXT).text_weight(700));
        header.add(
            Text::new("terminal renderer workload")
                .color(MUTED)
                .slot(Slot::new().width(Sizing::grow())),
        );
        for label in ["LIVE", "KITTY", "SGR"] {
            header.add(
                Text::new(label)
                    .color(ACCENT)
                    .slot(Slot::new().width(Sizing::fixed(48.0))),
            );
        }
    });
    root.add(|ui: &mut Ui| {
        let mut body = ui.layout(Flex::row().gap(12.0)).grow().open();
        body.add(|ui: &mut Ui| {
            let mut sidebar = ui
                .layout(Flex::column().padding(Sides::all(12.0)).gap(8.0))
                .width(Sizing::fixed(176.0))
                .height(Sizing::grow())
                .style(
                    Style::new()
                        .background(SURFACE)
                        .solid_border(2.0, SURFACE_HIGH)
                        .uniform_radius(6.0),
                )
                .open();
            sidebar.add(Text::new("WORKSPACES").color(MUTED).text_weight(700));
            for (index, label) in [
                "Overview",
                "Activity",
                "Processes",
                "Network",
                "Storage",
                "Metrics",
                "Tracing",
                "Alerts",
            ]
            .iter()
            .enumerate()
            {
                sidebar.add(|ui: &mut Ui| {
                    let id = WidgetId::new(("benchmark navigation", index));
                    let interaction = ui.interact(id, Sense::CLICK);
                    let active = index == state.selected;
                    let mut item = ui
                        .layout(Flex::row().padding(Sides::x(8.0)))
                        .id(id)
                        .width(Sizing::grow())
                        .height(Sizing::fixed(24.0))
                        .style(Style::new().background(
                            if active || interaction.hovered {
                                SURFACE_HIGH
                            } else {
                                SURFACE
                            },
                        ))
                        .open();
                    item.add(Text::new(if active { "›" } else { " " }).color(ACCENT));
                    item.add(Text::new(label).color(if active { TEXT } else { MUTED }));
                });
            }
        });
        body.add(|ui: &mut Ui| {
            let mut content = ui.layout(Flex::column().gap(12.0)).grow().open();
            content.add(|ui: &mut Ui| {
                let mut hero = ui
                    .layout(Flex::row().padding(Sides::all(12.0)).gap(12.0))
                    .width(Sizing::grow())
                    .height(Sizing::fixed(80.0))
                    .style(
                        Style::new()
                            .background(SURFACE)
                            .solid_border(2.0, ACCENT)
                            .uniform_radius(6.0),
                    )
                    .open();
                hero.add(|ui: &mut Ui| {
                    let mut copy = ui.layout(Flex::column().gap(6.0)).grow().open();
                    copy.add(
                        Text::new([
                            "Overview pipeline",
                            "Activity pipeline",
                            "Process pipeline",
                            "Network pipeline",
                            "Storage pipeline",
                            "Metrics pipeline",
                            "Tracing pipeline",
                            "Alert pipeline",
                        ][state.selected])
                        .color(TEXT)
                        .text_weight(700),
                    );
                    copy.add(
                        Text::new("Nested layout, styled text, retained images, borders, tables, and interaction damage in one representative frame.")
                            .color(MUTED)
                            .wrap(TextWrap::Word),
                    );
                });
                hero.add(
                    Image::new(image).slot(
                        Slot::new()
                            .width(Sizing::fixed(64.0))
                            .height(Sizing::fixed(48.0)),
                    ),
                );
            });
            content.add(|ui: &mut Ui| {
                let mut metrics = ui
                    .layout(Flex::row().gap(12.0))
                    .width(Sizing::grow())
                    .height(Sizing::fixed(64.0))
                    .open();
                for (value, label) in [
                    ("18.4 ms", "frame latency"),
                    ("2,481", "active spans"),
                    ("99.97%", "availability"),
                    ("42 MiB", "working set"),
                ] {
                    metrics.add(|ui: &mut Ui| {
                        let mut card = ui
                            .layout(Flex::column().padding(Sides::all(10.0)).gap(4.0))
                            .grow()
                            .style(
                                Style::new()
                                    .background(SURFACE_HIGH)
                                    .solid_border(2.0, SURFACE)
                                    .uniform_radius(4.0),
                            )
                            .open();
                        card.add(Text::new(value).color(TEXT).text_weight(700));
                        card.add(Text::new(label).color(MUTED));
                    });
                }
            });
            if state.expanded {
                content.add(
                    Text::new("Expanded incident context shifts the table and changes command structure.")
                        .color(ACCENT)
                        .slot(
                            Slot::new()
                                .width(Sizing::grow())
                                .height(Sizing::fixed(32.0)),
                        ),
                );
            }
            content.add(|ui: &mut Ui| {
                let mut table = ui
                    .layout(Flex::column().padding(Sides::all(10.0)).gap(4.0))
                    .grow()
                    .style(
                        Style::new()
                            .background(SURFACE)
                            .solid_border(2.0, SURFACE_HIGH)
                            .uniform_radius(6.0),
                    )
                    .open();
                for (index, (service, region, status, latency)) in [
                    ("gateway", "iad", "healthy", "8 ms"),
                    ("accounts", "fra", "healthy", "14 ms"),
                    ("billing", "syd", "degraded", "91 ms"),
                    ("search", "iad", "healthy", "22 ms"),
                    ("events", "sin", "healthy", "17 ms"),
                    ("storage", "fra", "healthy", "11 ms"),
                    ("workers", "syd", "healthy", "28 ms"),
                    ("telemetry", "sin", "degraded", "76 ms"),
                ]
                .iter()
                .enumerate()
                {
                    table.add(|ui: &mut Ui| {
                        let mut row = ui
                            .layout(Flex::row().align(Align::Center).gap(8.0).padding(Sides::x(6.0)))
                            .width(Sizing::grow())
                            .height(Sizing::fixed(22.0))
                            .style(Style::new().background(if index % 2 == 0 {
                                SURFACE_HIGH
                            } else {
                                SURFACE
                            }))
                            .open();
                        row.add(
                            Text::new(service)
                                .color(TEXT)
                                .slot(Slot::new().width(Sizing::grow())),
                        );
                        row.add(
                            Text::new(region)
                                .color(MUTED)
                                .slot(Slot::new().width(Sizing::fixed(48.0))),
                        );
                        row.add(
                            Text::new(status)
                                .color(if *status == "healthy" { ACCENT } else { MUTED })
                                .slot(Slot::new().width(Sizing::fixed(72.0))),
                        );
                        row.add(
                            Text::new(latency)
                                .color(TEXT)
                                .slot(Slot::new().width(Sizing::fixed(48.0))),
                        );
                    });
                }
            });
        });
    });
}

fn commands(changed: bool) -> CommandList {
    let mut commands = CommandList::default();
    commands.push_clear(screen());
    for row in 0..8 {
        for column in 0..12 {
            let area = LogicalRect {
                x: column as f32 * CELL_WIDTH * 8.0,
                y: row as f32 * CELL_HEIGHT * 4.0,
                width: CELL_WIDTH * 8.0,
                height: CELL_HEIGHT * 4.0,
            };
            let color = Color::from_rgba8(
                (column * 19) as u8,
                (row * 31) as u8,
                ((row + column) * 13) as u8,
                255,
            );
            commands.push_rectangle(
                Rectangle::new(area).background(color),
                area.to_physical(1.0),
                ClipId::default(),
            );
        }
    }
    if changed {
        let area = LogicalRect {
            x: 40.0 * CELL_WIDTH,
            y: 12.0 * CELL_HEIGHT,
            width: CELL_WIDTH,
            height: CELL_HEIGHT,
        };
        commands.push_rectangle(
            Rectangle::new(area).background(Color::from_rgba8(86, 211, 194, 255)),
            area.to_physical(1.0),
            ClipId::default(),
        );
    }
    commands
}

fn screen() -> PhysicalRect {
    PhysicalRect {
        x: 0,
        y: 0,
        width: i32::from(COLUMNS) * CELL_WIDTH as i32,
        height: i32::from(ROWS) * CELL_HEIGHT as i32,
    }
}
