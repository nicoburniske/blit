use std::time::Duration;

use blit::{
    Absolute, Anchor, Axis, Easing, Sense, Sides, Size, Sizing, Slot, Transition, Ui, Widget,
    WidgetId,
};
use blit_cpu::{Font, FontFace, RendererConfig};
use blit_desktop::{
    Application, BoundsClip, Config, DesktopPlatform, EventLoopProxy, Root,
    color::Color,
    draw::Rectangle,
    layout::{Align, Flex, Grid, Wrap},
    style::{Border, BorderRadius},
    text::{FontId, TextStyle},
    widget::Text,
};
use blit_showcase::{
    CanvasConfig, CanvasLayout, ITEMS, ItemSizing, Resizable, ResizeEdge, ResizeGrip, ResizeState,
};

fn main() {
    blit_desktop::run::<App>(Config {
        title: "Blit layout playground".into(),
        width: 1120,
        height: 800,
        renderer: RendererConfig {
            fonts: vec![FontFace {
                id: FontId::default(),
                weight: 400,
                font: Font::from_static(include_bytes!(env!("BLIT_TEST_FONT"))).unwrap(),
            }],
            font_metric_cache_capacity: 256,
            glyph_cache_capacity: 1024 * 1024,
            paragraph_cache_capacity: 2 * 1024 * 1024,
            shadow_cache_capacity: 64 * 1024,
        },
    })
    .unwrap();
}

struct App {
    canvas: CanvasConfig,
    resize: ResizeState,
}

impl Application for App {
    type Input = ();

    fn new(_: EventLoopProxy<Self::Input>, _: Root<Self>, _: &mut DesktopPlatform) -> Self {
        Self {
            canvas: CanvasConfig::default(),
            resize: ResizeState::default(),
        }
    }

    fn input(&mut self, _: Self::Input) {}

    fn render(&mut self, ui: &mut Ui<'_, DesktopPlatform>) {
        let screen = ui.screen();
        let unit = Size::new(8.0, 8.0);
        let mut root = ui.layout_with(
            Rectangle::new().background(colors::BACKGROUND),
            Flex::column().padding(Sides::all(20.0)).gap(14.0),
        );
        root.add(Slot::new().height(Sizing::fixed(52.0)), (), |mut ui| {
            let mut header = ui.layout(
                Flex::row()
                    .align(Align::Center)
                    .justify(blit_desktop::layout::Justify::SpaceBetween),
            );
            header.add(Slot::new().grow(), (), |mut ui| {
                let mut title = ui.layout(Flex::column().gap(2.0));
                title.add(Slot::new(), (), |mut ui| {
                    ui.add(
                        Text::new("BLIT / LAYOUT PLAYGROUND")
                            .style(TextStyle {
                                size: 22.0,
                                ..TextStyle::default()
                            })
                            .color(colors::TEXT),
                    );
                });
                title.add(Slot::new(), (), |mut ui| {
                    ui.add(
                        Text::new(
                            "shared layout and resize mechanics, native desktop presentation",
                        )
                        .style(TextStyle {
                            size: 11.0,
                            ..TextStyle::default()
                        })
                        .color(colors::TEXT_MUTED),
                    );
                });
            });
            if header.add(Slot::new(), (), |mut ui| {
                ui.add(Button::new(
                    WidgetId::new("reset layout playground"),
                    "RESET",
                    false,
                ))
            }) {
                self.canvas = CanvasConfig::default();
                self.resize.reset();
            }
        });
        root.add(Slot::new().grow(), (), |mut ui| {
            let mut body = ui.layout(Flex::row().gap(14.0));
            body.add(
                Slot::new()
                    .width(Sizing::fixed(330.0))
                    .height(Sizing::grow()),
                (),
                |mut ui| {
                    let mut controls = ui.layout_with(
                        panel(colors::SURFACE),
                        Flex::column().padding(Sides::all(14.0)).gap(7.0),
                    );
                    controls.add(Slot::new(), (), |mut ui| {
                        ui.add(
                            Text::new("LAYOUT PARAMETERS")
                                .style(TextStyle {
                                    size: 12.0,
                                    ..TextStyle::default()
                                })
                                .color(colors::ACCENT),
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            "layout",
                            "layout",
                            &mut self.canvas.layout,
                            &[
                                ("Flex", CanvasLayout::Flex),
                                ("Wrap", CanvasLayout::Wrap),
                                ("Grid", CanvasLayout::Grid),
                            ],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            "axis",
                            "axis",
                            &mut self.canvas.axis,
                            &[("Horizontal", Axis::Horizontal), ("Vertical", Axis::Vertical)],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            "justify",
                            "justify position",
                            &mut self.canvas.justify,
                            &[
                                ("Start", blit_desktop::layout::Justify::Start),
                                ("Center", blit_desktop::layout::Justify::Center),
                                ("End", blit_desktop::layout::Justify::End),
                            ],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            "space",
                            "justify distribution",
                            &mut self.canvas.justify,
                            &[
                                ("Between", blit_desktop::layout::Justify::SpaceBetween),
                                ("Around", blit_desktop::layout::Justify::SpaceAround),
                                ("Evenly", blit_desktop::layout::Justify::SpaceEvenly),
                            ],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            "align",
                            "align",
                            &mut self.canvas.align,
                            &[
                                ("Start", Align::Start),
                                ("Center", Align::Center),
                                ("End", Align::End),
                                ("Stretch", Align::Stretch),
                            ],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            "sizing",
                            "sizing",
                            &mut self.canvas.sizing,
                            &[
                                ("Fixed", ItemSizing::Fixed),
                                ("Fit", ItemSizing::Fit),
                                ("Grow", ItemSizing::Grow),
                            ],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            "zoom",
                            "zoom",
                            &mut self.canvas.zoom,
                            &[("75%", 0.75), ("100%", 1.0), ("125%", 1.25)],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            "gap",
                            "gap",
                            &mut self.canvas.gap_steps,
                            &[("0", 0), ("1", 1), ("2", 2), ("3", 3)],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            "padding",
                            "padding",
                            &mut self.canvas.padding_steps,
                            &[("0", 0), ("1", 1), ("2", 2), ("3", 3)],
                        );
                    });
                    controls.add(Slot::new(), (), |mut ui| {
                        choices(
                            &mut ui,
                            "transition",
                            "transitions",
                            &mut self.canvas.transitions,
                            &[("On", true), ("Off", false)],
                        );
                    });
                    controls.add(Slot::new().grow(), (), |mut ui| {
                        ui.add(
                            Text::new("Drag the highlighted right edge, bottom edge, or corner. Layout changes preserve item identity and animate geometry.")
                                .style(TextStyle {
                                    size: 11.0,
                                    ..TextStyle::default()
                                })
                                .color(colors::TEXT_DIM)
                                .options(blit_desktop::text::TextOptions {
                                    wrap: blit_desktop::text::TextWrap::Word,
                                    ..Default::default()
                                }),
                        );
                    });
                },
            );
            body.add(Slot::new().grow(), (), |mut ui| {
                let mut preview = ui.layout_with(
                    panel(colors::SURFACE),
                    Flex::column().padding(Sides::all(12.0)).gap(8.0),
                );
                preview.add(
                    Slot::new().height(Sizing::fixed(22.0)),
                    (),
                    |mut ui| {
                        ui.add(
                            Text::new("LIVE PREVIEW")
                                .style(TextStyle {
                                    size: 11.0,
                                    ..TextStyle::default()
                                })
                                .color(colors::ACCENT),
                        );
                    },
                );
                preview.add(Slot::new().grow(), (), |mut ui| {
                    let mut viewport = ui.layout_with(
                        Rectangle::new()
                            .background(colors::TRACK)
                            .radius(BorderRadius::uniform(8.0)),
                        Flex::row().padding(Sides::all(10.0)).align(Align::Start),
                    );
                    viewport.add(Slot::new(), (), |mut ui| {
                        ui.add(
                            Resizable::new(
                                &mut self.resize,
                                WidgetId::new("layout canvas"),
                                Size::new(
                                    (screen.width - 430.0).max(280.0) * 0.82,
                                    (screen.height - 150.0).max(220.0) * 0.78,
                                ),
                                Canvas {
                                    config: self.canvas,
                                    unit,
                                },
                                |grip: ResizeGrip| {
                                    let active = grip.interaction.hovered
                                        || grip.interaction.active
                                        || grip.interaction.dragging;
                                    Rectangle::new().background(if active {
                                        colors::ACCENT
                                    } else if grip.edge == ResizeEdge::Corner {
                                        colors::GRIP_CORNER
                                    } else {
                                        colors::GRIP
                                    })
                                },
                            )
                            .minimum(Size::new(240.0, 180.0))
                            .maximum(Size::new(
                                (screen.width - 390.0).max(240.0),
                                (screen.height - 120.0).max(180.0),
                            ))
                            .grip_size(Size::new(10.0, 10.0)),
                        );
                    });
                });
            });
        });
    }
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

impl Widget<DesktopPlatform> for Button<'_> {
    type Response = bool;

    fn build(self, ui: &mut Ui<'_, DesktopPlatform>) -> bool {
        let interaction = ui.interact(self.id, Sense::CLICK);
        let background = if interaction.active {
            colors::ACCENT_DARK
        } else if self.selected {
            colors::SELECTED
        } else if interaction.hovered {
            colors::SURFACE_HIGH
        } else {
            colors::TRACK
        };
        let border = if self.selected {
            colors::ACCENT
        } else {
            colors::BORDER
        };
        let mut button = ui
            .layout_with(
                Rectangle::new()
                    .background(background)
                    .border(Border::solid(1.0, border))
                    .radius(BorderRadius::uniform(5.0)),
                Flex::row().padding(Sides::new().top(5.0).right(8.0).bottom(5.0).left(8.0)),
            )
            .id(self.id);
        button.add(Slot::new(), (), |mut ui| {
            ui.add(
                Text::new(self.label)
                    .style(TextStyle {
                        size: 10.0,
                        ..TextStyle::default()
                    })
                    .color(colors::TEXT),
            );
        });
        interaction.clicked
    }
}

fn choices<T: Copy + PartialEq>(
    ui: &mut Ui<'_, DesktopPlatform>,
    label: &str,
    id: &str,
    selected: &mut T,
    options: &[(&str, T)],
) {
    let mut line = ui.layout(Flex::row().align(Align::Center).gap(6.0));
    line.add(Slot::new().width(Sizing::fixed(66.0)), (), |mut ui| {
        ui.add(
            Text::new(label)
                .style(TextStyle {
                    size: 10.0,
                    ..TextStyle::default()
                })
                .color(colors::TEXT_MUTED),
        );
    });
    line.add(Slot::new().grow(), (), |mut ui| {
        let mut values = ui.layout(Flex::row().gap(4.0));
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

impl Widget<DesktopPlatform> for Canvas {
    type Response = ();

    fn build(self, ui: &mut Ui<'_, DesktopPlatform>) {
        let background = Rectangle::new()
            .background(colors::CANVAS)
            .border(Border::solid(2.0, colors::CANVAS_BORDER))
            .radius(BorderRadius::uniform(8.0));
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
                            .height(Sizing::fixed(5.0 * self.unit.height * self.config.zoom)),
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
    ui: &mut Ui<'_, DesktopPlatform>,
    index: usize,
    spec: blit_showcase::ItemSpec,
    badges: blit::LayerId,
    config: CanvasConfig,
) {
    let item = ui.layout_with(
        Rectangle::new()
            .background(colors::ITEMS[index])
            .radius(BorderRadius::uniform(5.0)),
        Flex::column()
            .align(Align::Center)
            .justify(blit_desktop::layout::Justify::Center),
    );
    let mut item = if config.transitions {
        item.id(WidgetId::new(("canvas item", index))).transition(
            Transition::new(Duration::from_millis(320))
                .easing(Easing::EaseOutQuad)
                .layout(),
        )
    } else {
        item
    };
    item.add(Slot::new(), (), |mut ui| {
        ui.add(
            Text::new(spec.label)
                .style(TextStyle {
                    size: 11.0 * config.zoom,
                    ..TextStyle::default()
                })
                .color(Color::WHITE),
        );
    });
    if let Some(anchor) = spec.badge {
        item.add(
            Slot::new()
                .fixed(28.0 * config.zoom, 14.0 * config.zoom)
                .layer(badges)
                .z_index(1),
            (),
            |mut ui| {
                let mut badge = ui
                    .layout_with(
                        Rectangle::new()
                            .background(colors::BACKGROUND)
                            .border(Border::solid(1.0, Color::WHITE))
                            .radius(BorderRadius::uniform(4.0)),
                        Flex::row()
                            .align(Align::Center)
                            .justify(blit_desktop::layout::Justify::Center),
                    )
                    .absolute(Absolute::attach(anchor, Anchor::Center));
                badge.add(Slot::new(), (), |mut ui| {
                    ui.add(
                        Text::new("ABS")
                            .style(TextStyle {
                                size: (7.0 * config.zoom).max(7.0),
                                ..TextStyle::default()
                            })
                            .color(Color::WHITE),
                    );
                });
            },
        );
    }
}

fn panel(background: Color) -> Rectangle {
    Rectangle::new()
        .background(background)
        .border(Border::solid(1.0, colors::BORDER))
        .radius(BorderRadius::uniform(10.0))
}

mod colors {
    use blit_desktop::color::Color;

    pub const BACKGROUND: Color = Color::from_rgba8(12, 18, 29, 255);
    pub const SURFACE: Color = Color::from_rgba8(20, 29, 45, 255);
    pub const SURFACE_HIGH: Color = Color::from_rgba8(38, 53, 77, 255);
    pub const TRACK: Color = Color::from_rgba8(9, 15, 25, 255);
    pub const SELECTED: Color = Color::from_rgba8(27, 87, 82, 255);
    pub const CANVAS: Color = Color::from_rgba8(25, 36, 54, 255);
    pub const CANVAS_BORDER: Color = Color::from_rgba8(68, 91, 123, 255);
    pub const GRIP: Color = Color::from_rgba8(46, 77, 101, 255);
    pub const GRIP_CORNER: Color = Color::from_rgba8(65, 119, 133, 255);
    pub const BORDER: Color = Color::from_rgba8(55, 72, 99, 255);
    pub const TEXT: Color = Color::from_rgba8(235, 242, 250, 255);
    pub const TEXT_MUTED: Color = Color::from_rgba8(157, 173, 194, 255);
    pub const TEXT_DIM: Color = Color::from_rgba8(106, 126, 151, 255);
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
