use std::{io, time::Duration};

use blit::{
    Absolute, Anchor, Axis, Easing, Input, Key, Sense, Sides, Size, Sizing, Slot, Transition, Ui,
    Widget, WidgetId,
};
use blit_showcase::{
    CanvasConfig, CanvasLayout, ITEMS, ItemSizing, Resizable, ResizeEdge, ResizeGrip, ResizeState,
};
use blit_terminal::{
    BoundsClip, ControlFlow, TerminalPlatform,
    color::Color,
    draw::{Block, Border},
    layout::{Align, Flex, Grid, Justify, Wrap},
    widget::Text,
};

fn main() -> io::Result<()> {
    let mut canvas = CanvasConfig::default();
    let mut resize = ResizeState::default();
    blit_terminal::run(|ui: &mut Ui<'_, TerminalPlatform>| {
        let control = if matches!(ui.input(), Input::Text('q'))
            || matches!(ui.input(), Input::Key(key) if key.key == Key::Escape)
        {
            ControlFlow::Exit
        } else {
            ControlFlow::Continue
        };
        let screen = ui.screen();
        let cell = ui.platform().renderer().cell_size();
        let mut root = ui.layout_with(
            Block::new().background(colors::BACKGROUND),
            Flex::column()
                .padding(Sides {
                    top: cell.height,
                    right: cell.width,
                    bottom: cell.height,
                    left: cell.width,
                })
                .gap(cell.height),
        );
        root.add(
            Slot::new().height(Sizing::fixed(cell.height)),
            (),
            |mut ui| {
                let mut header = ui.layout(
                    Flex::row()
                        .align(Align::Center)
                        .justify(Justify::SpaceBetween),
                );
                header.add(Slot::new().grow(), (), |mut ui| {
                    ui.add(Text::new("BLIT / LAYOUT PLAYGROUND").bold(true));
                });
                if header.add(Slot::new(), (), |mut ui| {
                    ui.add(Button::new(
                        WidgetId::new("terminal reset layout playground"),
                        "RESET",
                        false,
                    ))
                }) {
                    canvas = CanvasConfig::default();
                    resize.reset();
                }
            },
        );
        root.add(Slot::new().grow(), (), |mut ui| {
            let mut body = ui.layout(Flex::row().gap(cell.width));
            body.add(
                Slot::new()
                    .width(Sizing::fixed(cell.width * 30.0))
                    .height(Sizing::grow()),
                (),
                |mut ui| {
                    let mut controls = ui.layout_with(
                        panel(colors::SURFACE),
                        Flex::column().padding(Sides {
                            top: cell.height,
                            right: cell.width,
                            bottom: cell.height,
                            left: cell.width,
                        }),
                    );
                    controls.add(
                        Slot::new().height(Sizing::fixed(cell.height)),
                        (),
                        |mut ui| {
                            ui.add(
                                Text::new("LAYOUT PARAMETERS")
                                    .color(colors::ACCENT)
                                    .bold(true),
                            );
                        },
                    );
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            cell,
                            "layout",
                            "terminal layout",
                            &mut canvas.layout,
                            &[
                                ("F", CanvasLayout::Flex),
                                ("W", CanvasLayout::Wrap),
                                ("G", CanvasLayout::Grid),
                            ],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            cell,
                            "axis",
                            "terminal axis",
                            &mut canvas.axis,
                            &[("H", Axis::Horizontal), ("V", Axis::Vertical)],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            cell,
                            "justify",
                            "terminal justify position",
                            &mut canvas.justify,
                            &[
                                ("S", Justify::Start),
                                ("C", Justify::Center),
                                ("E", Justify::End),
                            ],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            cell,
                            "space",
                            "terminal justify distribution",
                            &mut canvas.justify,
                            &[
                                ("B", Justify::SpaceBetween),
                                ("A", Justify::SpaceAround),
                                ("V", Justify::SpaceEvenly),
                            ],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            cell,
                            "align",
                            "terminal align",
                            &mut canvas.align,
                            &[
                                ("S", Align::Start),
                                ("C", Align::Center),
                                ("E", Align::End),
                                ("X", Align::Stretch),
                            ],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            cell,
                            "sizing",
                            "terminal sizing",
                            &mut canvas.sizing,
                            &[
                                ("Fix", ItemSizing::Fixed),
                                ("Fit", ItemSizing::Fit),
                                ("Gr", ItemSizing::Grow),
                            ],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            cell,
                            "zoom",
                            "terminal zoom",
                            &mut canvas.zoom,
                            &[("75", 0.75), ("100", 1.0), ("125", 1.25)],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            cell,
                            "gap",
                            "terminal gap",
                            &mut canvas.gap_steps,
                            &[("0", 0), ("1", 1), ("2", 2), ("3", 3)],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            cell,
                            "padding",
                            "terminal padding",
                            &mut canvas.padding_steps,
                            &[("0", 0), ("1", 1), ("2", 2), ("3", 3)],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            cell,
                            "trans",
                            "terminal transitions",
                            &mut canvas.transitions,
                            &[("On", true), ("Off", false)],
                        );
                    });
                },
            );
            body.add(Slot::new().grow(), (), |mut ui| {
                let mut preview = ui.layout_with(
                    panel(colors::SURFACE),
                    Flex::column().padding(Sides {
                        top: cell.height,
                        right: cell.width,
                        bottom: cell.height,
                        left: cell.width,
                    }),
                );
                preview.add(
                    Slot::new().height(Sizing::fixed(cell.height)),
                    (),
                    |mut ui| {
                        ui.add(Text::new("LIVE PREVIEW").color(colors::ACCENT).bold(true));
                    },
                );
                preview.add(Slot::new().grow(), (), |mut ui| {
                    let mut viewport = ui.layout_with(
                        Block::new().background(colors::TRACK),
                        Flex::row()
                            .padding(Sides {
                                top: cell.height,
                                right: cell.width,
                                bottom: cell.height,
                                left: cell.width,
                            })
                            .align(Align::Start),
                    );
                    viewport.add(Slot::new(), (), |mut ui| {
                        ui.add(
                            Resizable::new(
                                &mut resize,
                                WidgetId::new("terminal layout canvas"),
                                Size::new(
                                    (screen.width - cell.width * 34.0).max(cell.width * 16.0),
                                    (screen.height - cell.height * 6.0).max(cell.height * 7.0),
                                ),
                                Canvas {
                                    config: canvas,
                                    unit: cell,
                                },
                                |grip: ResizeGrip| {
                                    let active = grip.interaction.hovered
                                        || grip.interaction.active
                                        || grip.interaction.dragging;
                                    Block::new().background(if active {
                                        colors::ACCENT
                                    } else if grip.edge == ResizeEdge::Corner {
                                        colors::GRIP_CORNER
                                    } else {
                                        colors::GRIP
                                    })
                                },
                            )
                            .minimum(Size::new(cell.width * 16.0, cell.height * 7.0))
                            .maximum(screen.size())
                            .grip_size(cell),
                        );
                    });
                });
            });
        });
        control
    })
}

struct Button<'a> {
    id: WidgetId,
    label: &'a str,
    selected: bool,
}

impl<'a> Button<'a> {
    fn new(id: WidgetId, label: &'a str, selected: bool) -> Self {
        Self {
            id,
            label,
            selected,
        }
    }
}

impl Widget<TerminalPlatform> for Button<'_> {
    type Response = bool;

    fn build(self, ui: &mut Ui<'_, TerminalPlatform>) -> bool {
        let interaction = ui.interact(self.id, Sense::CLICK);
        let mut block = Block::new().background(if interaction.active {
            colors::ACCENT_DARK
        } else if self.selected {
            colors::SELECTED
        } else if interaction.hovered {
            colors::SURFACE_HIGH
        } else {
            colors::TRACK
        });
        if self.selected {
            block = block.border(Border::new(colors::ACCENT).rounded(true));
        }
        let mut button = ui.layout_with(block, Flex::row()).id(self.id);
        button.add(Slot::new(), (), |mut ui| {
            ui.add(Text::new(self.label).color(colors::TEXT));
        });
        interaction.clicked
    }
}

fn choices<T: Copy + PartialEq>(
    ui: &mut Ui<'_, TerminalPlatform>,
    cell: Size,
    label: &str,
    id: &str,
    selected: &mut T,
    options: &[(&str, T)],
) {
    let mut line = ui.layout(Flex::row().align(Align::Center).gap(cell.width));
    line.add(
        Slot::new().width(Sizing::fixed(cell.width * 8.0)),
        (),
        |mut ui| {
            ui.add(Text::new(label).color(colors::TEXT_MUTED));
        },
    );
    line.add(Slot::new().grow(), (), |mut ui| {
        let mut values = ui.layout(Flex::row().gap(cell.width));
        for (index, &(option, value)) in options.iter().enumerate() {
            let clicked = values.add(Slot::new(), (), |mut ui| {
                ui.add(Button::new(
                    WidgetId::new((id, index)),
                    option,
                    *selected == value,
                ))
            });
            if clicked {
                *selected = value;
            }
        }
    });
}

#[derive(Clone, Copy)]
struct Canvas {
    config: CanvasConfig,
    unit: Size,
}

impl Widget<TerminalPlatform> for Canvas {
    type Response = ();

    fn build(self, ui: &mut Ui<'_, TerminalPlatform>) {
        let background = Block::new()
            .background(colors::CANVAS)
            .border(Border::new(colors::CANVAS_BORDER).rounded(true));
        match self.config.layout {
            CanvasLayout::Flex => {
                let mut canvas = ui
                    .layout_with(
                        background,
                        Flex::new(self.config.axis)
                            .padding(self.config.padding(self.unit))
                            .gap(self.config.gap(self.config.axis, self.unit))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .clip(BoundsClip);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    canvas.add(self.config.item_slot(index, self.unit), (), |mut ui| {
                        canvas_item(&mut ui, index, spec, badges, self.config);
                    });
                }
            }
            CanvasLayout::Wrap => {
                let cross = match self.config.axis {
                    Axis::Horizontal => Axis::Vertical,
                    Axis::Vertical => Axis::Horizontal,
                };
                let mut canvas = ui
                    .layout_with(
                        background,
                        Wrap::new(self.config.axis)
                            .padding(self.config.padding(self.unit))
                            .item_gap(self.config.gap(self.config.axis, self.unit))
                            .run_gap(self.config.gap(cross, self.unit))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .clip(BoundsClip);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    canvas.add(self.config.item_slot(index, self.unit), (), |mut ui| {
                        canvas_item(&mut ui, index, spec, badges, self.config);
                    });
                }
            }
            CanvasLayout::Grid => {
                let grid = Grid::columns(5)
                    .spanning()
                    .padding(self.config.padding(self.unit))
                    .column_gap(self.config.gap(Axis::Horizontal, self.unit))
                    .row_gap(self.config.gap(Axis::Vertical, self.unit));
                let mut placer = grid.placer();
                let mut canvas = ui.layout_with(background, grid).clip(BoundsClip);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    let item = placer.place(spec.rows, spec.columns);
                    canvas.add(
                        Slot::new()
                            .height(Sizing::fixed(3.0 * self.unit.height * self.config.zoom)),
                        item,
                        |mut ui| {
                            canvas_item(&mut ui, index, spec, badges, self.config);
                        },
                    );
                }
            }
        }
    }
}

fn canvas_item(
    ui: &mut Ui<'_, TerminalPlatform>,
    index: usize,
    spec: blit_showcase::ItemSpec,
    badges: blit::LayerId,
    config: CanvasConfig,
) {
    let item = ui.layout_with(
        Block::new().background(colors::ITEMS[index]),
        Flex::column().align(Align::Center).justify(Justify::Center),
    );
    let mut item = if config.transitions {
        item.id(WidgetId::new(("terminal canvas item", index)))
            .transition(
                Transition::new(Duration::from_millis(320))
                    .easing(Easing::EaseOutQuad)
                    .layout(),
            )
    } else {
        item
    };
    item.add(Slot::new(), (), |mut ui| {
        ui.add(Text::new(spec.label).color(colors::TEXT).bold(true));
    });
    if let Some(anchor) = spec.badge {
        item.add(
            Slot::new().fixed(3.0, 1.0).layer(badges).z_index(1),
            (),
            |mut ui| {
                let mut badge = ui
                    .layout_with(
                        Block::new()
                            .background(colors::BACKGROUND)
                            .border(Border::new(colors::TEXT).rounded(true)),
                        Flex::row().align(Align::Center).justify(Justify::Center),
                    )
                    .absolute(Absolute::attach(anchor, Anchor::Center));
                badge.add(Slot::new(), (), |mut ui| {
                    ui.add(Text::new("A").color(colors::TEXT));
                });
            },
        );
    }
}

fn panel(background: Color) -> Block {
    Block::new()
        .background(background)
        .border(Border::new(colors::BORDER).rounded(true))
}

mod colors {
    use blit_terminal::color::Color;

    pub const BACKGROUND: Color = Color::from_rgba8(8, 12, 20, 255);
    pub const SURFACE: Color = Color::from_rgba8(18, 27, 42, 255);
    pub const SURFACE_HIGH: Color = Color::from_rgba8(35, 51, 74, 255);
    pub const TRACK: Color = Color::from_rgba8(5, 9, 16, 255);
    pub const SELECTED: Color = Color::from_rgba8(24, 84, 78, 255);
    pub const CANVAS: Color = Color::from_rgba8(24, 35, 52, 255);
    pub const CANVAS_BORDER: Color = Color::from_rgba8(74, 99, 132, 255);
    pub const GRIP: Color = Color::from_rgba8(44, 73, 98, 255);
    pub const GRIP_CORNER: Color = Color::from_rgba8(62, 112, 127, 255);
    pub const BORDER: Color = Color::from_rgba8(54, 72, 98, 255);
    pub const TEXT: Color = Color::from_rgba8(235, 242, 250, 255);
    pub const TEXT_MUTED: Color = Color::from_rgba8(150, 169, 192, 255);
    pub const ACCENT: Color = Color::from_rgba8(91, 220, 185, 255);
    pub const ACCENT_DARK: Color = Color::from_rgba8(31, 111, 104, 255);
    pub const ITEMS: [Color; 10] = [
        Color::from_rgba8(73, 135, 218, 255),
        Color::from_rgba8(53, 174, 126, 255),
        Color::from_rgba8(224, 142, 62, 255),
        Color::from_rgba8(146, 92, 220, 255),
        Color::from_rgba8(218, 89, 143, 255),
        Color::from_rgba8(47, 162, 184, 255),
        Color::from_rgba8(111, 148, 68, 255),
        Color::from_rgba8(205, 103, 73, 255),
        Color::from_rgba8(112, 103, 209, 255),
        Color::from_rgba8(191, 79, 119, 255),
    ];
}
