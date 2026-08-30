use std::{io, time::Duration};

use blit::{
    Absolute, Anchor, Axis, Easing, Input, Key, Sense, Sides, Size, Sizing, Slot, Transition,
    Widget, WidgetId,
};
use blit_showcase::{
    CanvasConfig, CanvasLayout, ITEMS, ItemSizing, Resizable, ResizeEdge, ResizeGrip, ResizeState,
};
use blit_terminal::{
    BoundsClip, ControlFlow, TerminalPlatform, Ui,
    color::Color,
    draw::{Block, Border},
    layout::{Align, Flex, Grid, Justify, Wrap},
    widget::Text,
};

fn main() -> io::Result<()> {
    let mut canvas = CanvasConfig::default();
    let mut resize = ResizeState::default();
    blit_terminal::run(|ui: &mut Ui<'_>| {
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
        root.child()
            .slot(Slot::new().height(Sizing::fixed(cell.height)))
            .layout(
                Flex::row().align(Align::Center).gap(cell.width * 2.0),
                |mut header| {
                    header.add(Text::new("BLIT / LAYOUT PLAYGROUND").bold(true));
                    if header.add(Button::new(
                        WidgetId::new("terminal reset layout playground"),
                        " Reset ",
                        false,
                    )) {
                        canvas = CanvasConfig::default();
                        resize.reset();
                    }
                    header.add(Text::new("q quit").color(colors::TEXT_MUTED));
                },
            );
        root.child()
            .slot(Slot::new().grow())
            .layout(Flex::row().gap(cell.width), |mut body| {
                body.child()
                    .slot(
                        Slot::new()
                            .width(Sizing::fixed(cell.width * 40.0))
                            .height(Sizing::grow()),
                    )
                    .layout_with(
                        panel(colors::SURFACE),
                        Flex::column()
                            .padding(Sides {
                                top: cell.height,
                                right: cell.width,
                                bottom: cell.height,
                                left: cell.width,
                            })
                            .gap(cell.height),
                        |mut controls| {
                            controls
                                .child()
                                .slot(Slot::new().height(Sizing::fixed(cell.height)))
                                .add(
                                    Text::new("LAYOUT PARAMETERS")
                                        .color(colors::ACCENT)
                                        .bold(true),
                                );
                            controls.add(Text::new("FLOW").color(colors::TEXT_DIM).bold(true));
                            controls.add(|ui: &mut Ui<'_>| {
                                choices(
                                    ui,
                                    cell,
                                    "layout",
                                    "terminal layout",
                                    &mut canvas.layout,
                                    &[
                                        (" Flex ", CanvasLayout::Flex),
                                        (" Wrap ", CanvasLayout::Wrap),
                                        (" Grid ", CanvasLayout::Grid),
                                    ],
                                );
                            });
                            controls.add(|ui: &mut Ui<'_>| {
                                choices(
                                    ui,
                                    cell,
                                    "axis",
                                    "terminal axis",
                                    &mut canvas.axis,
                                    &[(" Horz ", Axis::Horizontal), (" Vert ", Axis::Vertical)],
                                );
                            });
                            controls
                                .add(Text::new("DISTRIBUTION").color(colors::TEXT_DIM).bold(true));
                            controls.add(|ui: &mut Ui<'_>| {
                                choices(
                                    ui,
                                    cell,
                                    "justify",
                                    "terminal justify position",
                                    &mut canvas.justify,
                                    &[
                                        (" Start ", Justify::Start),
                                        (" Center ", Justify::Center),
                                        (" End ", Justify::End),
                                    ],
                                );
                            });
                            controls.add(|ui: &mut Ui<'_>| {
                                choices(
                                    ui,
                                    cell,
                                    "distribute",
                                    "terminal justify distribution",
                                    &mut canvas.justify,
                                    &[
                                        (" Between ", Justify::SpaceBetween),
                                        (" Around ", Justify::SpaceAround),
                                        (" Even ", Justify::SpaceEvenly),
                                    ],
                                );
                            });
                            controls.add(|ui: &mut Ui<'_>| {
                                choices(
                                    ui,
                                    cell,
                                    "align",
                                    "terminal align",
                                    &mut canvas.align,
                                    &[
                                        (" Start ", Align::Start),
                                        (" Center ", Align::Center),
                                        (" End ", Align::End),
                                        (" Stretch ", Align::Stretch),
                                    ],
                                );
                            });
                            controls.add(
                                Text::new("SCALE, SPACE & MOTION")
                                    .color(colors::TEXT_DIM)
                                    .bold(true),
                            );
                            controls.add(|ui: &mut Ui<'_>| {
                                choices(
                                    ui,
                                    cell,
                                    "sizing",
                                    "terminal sizing",
                                    &mut canvas.sizing,
                                    &[
                                        (" Fixed ", ItemSizing::Fixed),
                                        (" Fit ", ItemSizing::Fit),
                                        (" Grow ", ItemSizing::Grow),
                                    ],
                                );
                            });
                            controls.add(|ui: &mut Ui<'_>| {
                                choices(
                                    ui,
                                    cell,
                                    "zoom",
                                    "terminal zoom",
                                    &mut canvas.zoom,
                                    &[(" 75% ", 0.75), (" 100% ", 1.0), (" 125% ", 1.25)],
                                );
                            });
                            controls.add(|ui: &mut Ui<'_>| {
                                choices(
                                    ui,
                                    cell,
                                    "gap",
                                    "terminal gap",
                                    &mut canvas.gap_steps,
                                    &[(" 0 ", 0), (" 1 ", 1), (" 2 ", 2), (" 3 ", 3)],
                                );
                            });
                            controls.add(|ui: &mut Ui<'_>| {
                                choices(
                                    ui,
                                    cell,
                                    "padding",
                                    "terminal padding",
                                    &mut canvas.padding_steps,
                                    &[(" 0 ", 0), (" 1 ", 1), (" 2 ", 2), (" 3 ", 3)],
                                );
                            });
                            controls.add(|ui: &mut Ui<'_>| {
                                choices(
                                    ui,
                                    cell,
                                    "transitions",
                                    "terminal transitions",
                                    &mut canvas.transitions,
                                    &[(" On ", true), (" Off ", false)],
                                );
                            });
                            controls.child().slot(Slot::new().grow()).layout(
                                Flex::column().justify(Justify::End),
                                |mut help| {
                                    help.add(
                                        Text::new("POINTER").color(colors::TEXT_DIM).bold(true),
                                    );
                                    help.add(
                                        Text::new("click a value to select it")
                                            .color(colors::TEXT_MUTED),
                                    );
                                    help.add(
                                        Text::new("drag the preview handles to resize")
                                            .color(colors::TEXT_MUTED),
                                    );
                                },
                            );
                        },
                    );
                body.child().slot(Slot::new().grow()).layout_with(
                    panel(colors::SURFACE),
                    Flex::column().padding(Sides {
                        top: cell.height,
                        right: cell.width,
                        bottom: cell.height,
                        left: cell.width,
                    }),
                    |mut preview| {
                        preview
                            .child()
                            .slot(Slot::new().height(Sizing::fixed(cell.height)))
                            .layout(Flex::row().gap(cell.width), |mut status| {
                                status.add(
                                    Text::new("LIVE PREVIEW").color(colors::ACCENT).bold(true),
                                );
                                status.add(Text::new("/").color(colors::TEXT_DIM));
                                status.add(
                                    Text::new(match canvas.layout {
                                        CanvasLayout::Flex => "flex",
                                        CanvasLayout::Wrap => "wrap",
                                        CanvasLayout::Grid => "grid",
                                    })
                                    .color(colors::TEXT_MUTED),
                                );
                                status.add(
                                    Text::new(match canvas.axis {
                                        Axis::Horizontal => "horizontal",
                                        Axis::Vertical => "vertical",
                                    })
                                    .color(colors::TEXT_MUTED),
                                );
                            });
                        preview.child().slot(Slot::new().grow()).layout_with(
                            Block::new().background(colors::TRACK),
                            Flex::row()
                                .padding(Sides {
                                    top: cell.height,
                                    right: cell.width,
                                    bottom: cell.height,
                                    left: cell.width,
                                })
                                .align(Align::Start),
                            |mut viewport| {
                                viewport.add(
                                    Resizable::new(
                                        &mut resize,
                                        WidgetId::new("terminal layout canvas"),
                                        Size::new(
                                            ((screen.width - cell.width * 48.0) * 0.8)
                                                .max(cell.width * 18.0),
                                            ((screen.height - cell.height * 8.0) * 0.72)
                                                .max(cell.height * 9.0),
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
                                    .minimum(Size::new(cell.width * 18.0, cell.height * 9.0))
                                    .maximum(screen.size())
                                    .grip_size(cell),
                                );
                            },
                        );
                        preview
                            .child()
                            .slot(Slot::new().height(Sizing::fixed(cell.height)))
                            .add(
                                Text::new("drag the teal edge or corner to resize")
                                    .color(colors::TEXT_DIM),
                            );
                    },
                );
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

    fn build(self, ui: &mut Ui<'_>) -> bool {
        let interaction = ui.interact(self.id, Sense::CLICK);
        let block = if interaction.active {
            Block::new().background(colors::ACCENT_DARK)
        } else if self.selected {
            Block::new().background(colors::SELECTED)
        } else if interaction.hovered {
            Block::new().background(colors::SURFACE_HIGH)
        } else {
            Block::new()
        };
        let mut button = ui.layout_with(block, Flex::row()).id(self.id);
        button.add(Text::new(self.label).color(colors::TEXT));
        interaction.clicked
    }
}

fn choices<T: Copy + PartialEq>(
    ui: &mut Ui<'_>,
    cell: Size,
    label: &str,
    id: &str,
    selected: &mut T,
    options: &[(&str, T)],
) {
    let mut group = ui.layout(Flex::column());
    group.add(Text::new(label).color(colors::TEXT_MUTED));
    group.add(|ui: &mut Ui<'_>| {
        let mut values = ui.layout(
            Wrap::new(Axis::Horizontal)
                .item_gap(cell.width * 2.0)
                .run_gap(cell.height)
                .align(Align::Center),
        );
        for (index, &(option, value)) in options.iter().enumerate() {
            let clicked = values.add(Button::new(
                WidgetId::new((id, index)),
                option,
                *selected == value,
            ));
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

    fn build(self, ui: &mut Ui<'_>) {
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
                    canvas
                        .child()
                        .slot(self.config.item_slot(index, self.unit))
                        .add(|ui: &mut Ui<'_>| {
                            canvas_item(ui, index, spec, badges, self.config, self.unit);
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
                    canvas
                        .child()
                        .slot(self.config.item_slot(index, self.unit))
                        .add(|ui: &mut Ui<'_>| {
                            canvas_item(ui, index, spec, badges, self.config, self.unit);
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
                    canvas
                        .item(item)
                        .slot(
                            Slot::new()
                                .height(Sizing::fixed(3.0 * self.unit.height * self.config.zoom)),
                        )
                        .add(|ui: &mut Ui<'_>| {
                            canvas_item(ui, index, spec, badges, self.config, self.unit);
                        });
                }
            }
        }
    }
}

fn canvas_item(
    ui: &mut Ui<'_>,
    index: usize,
    spec: blit_showcase::ItemSpec,
    badges: blit::LayerId,
    config: CanvasConfig,
    unit: Size,
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
    item.add(Text::new(spec.label).color(colors::TEXT).bold(true));
    if let Some(anchor) = spec.badge {
        item.child()
            .slot(
                Slot::new()
                    .fixed(unit.width * 2.0, unit.height)
                    .layer(badges)
                    .z_index(1),
            )
            .add(|ui: &mut Ui<'_>| {
                let mut badge = ui
                    .layout_with(
                        Block::new().background(colors::ACCENT_DARK),
                        Flex::row().align(Align::Center).justify(Justify::Center),
                    )
                    .absolute(Absolute::attach(anchor, Anchor::Center));
                badge.add(Text::new("A").color(colors::TEXT));
            });
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
    pub const GRIP: Color = Color::from_rgba8(48, 126, 120, 255);
    pub const GRIP_CORNER: Color = Color::from_rgba8(91, 220, 185, 255);
    pub const BORDER: Color = Color::from_rgba8(54, 72, 98, 255);
    pub const TEXT: Color = Color::from_rgba8(235, 242, 250, 255);
    pub const TEXT_MUTED: Color = Color::from_rgba8(150, 169, 192, 255);
    pub const TEXT_DIM: Color = Color::from_rgba8(93, 113, 139, 255);
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
