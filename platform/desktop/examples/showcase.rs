use std::{fmt::Write as _, time::Duration};

use blit::{
    Absolute, Anchor, Axis, Easing, NodeId, Place, Sense, Sides, Size, Sizing, Transition, Widget,
    WidgetId,
};
use blit_cpu::{Font, FontFace, RendererConfig};
use blit_desktop::{
    Application, BoundsClip, Config, Cx, DesktopPlatform, EventLoopProxy, Root, Ui,
    atom::Rectangle,
    color::Color,
    layout::{Align, Flex, Grid, Wrap},
    style::{Border, BorderRadius},
    text::{FontId, TextStyle},
    widget::{Text, scroll},
};
use blit_showcase::{
    CanvasConfig, CanvasLayout, FpsCounter, ITEMS, ItemSizing, Resizable, ResizeEdge, ResizeGrip,
    ResizeState,
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

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Layout,
    Scroll,
}

struct App {
    page: Page,
    canvas: CanvasConfig,
    resize: ResizeState,
    scroll_axis: Axis,
    scroll: scroll::State,
    fps: FpsBadge,
}

impl Application for App {
    type Input = ();

    fn new(_: EventLoopProxy<Self::Input>, _: Root<Self>, _: &mut DesktopPlatform) -> Self {
        Self {
            page: Page::default(),
            canvas: CanvasConfig::default(),
            resize: ResizeState::default(),
            scroll_axis: Axis::Vertical,
            scroll: scroll::State::default(),
            fps: FpsBadge::default(),
        }
    }

    fn input(&mut self, _: Self::Input) {}

    fn render(&mut self, ui: &mut Ui) {
        let screen = ui.screen();
        let unit = Size::new(8.0, 8.0);
        let mut root = ui
            .node(Flex::column().padding(Sides::all(20.0)).gap(14.0))
            .insert(Rectangle::new().background(colors::BACKGROUND));
        root.place(Place::new().height(Sizing::fixed(52.0)))
            .add(|ui: Cx<'_>| {
                let mut header = ui.node(
                    Flex::row()
                        .align(Align::Center)
                        .justify(blit_desktop::layout::Justify::SpaceBetween),
                );
                header.place(Place::new().grow()).add(|ui: Cx<'_>| {
                    let mut title = ui.node(Flex::column().gap(2.0));
                    title.add(
                        Text::new("BLIT / SHOWCASE")
                            .style(TextStyle {
                                size: 22.0,
                                ..TextStyle::default()
                            })
                            .color(colors::TEXT),
                    );
                    title.add(
                        Text::new("layout, scrolling, and native desktop presentation")
                            .style(TextStyle {
                                size: 11.0,
                                ..TextStyle::default()
                            })
                            .color(colors::TEXT_MUTED),
                    );
                });
                for (page, label) in [(Page::Layout, "LAYOUT"), (Page::Scroll, "SCROLL")] {
                    if header.add(Button::new(
                        WidgetId::new(("desktop page", label)),
                        label,
                        self.page == page,
                    )) {
                        self.page = page;
                    }
                }
                if header.add(Button::new(
                    WidgetId::new("reset desktop showcase"),
                    "RESET",
                    false,
                )) {
                    self.canvas = CanvasConfig::default();
                    self.resize.reset();
                    self.scroll_axis = Axis::Vertical;
                    self.scroll = scroll::State::default();
                }
            });
        if self.page == Page::Layout {
            let mut body = root.place(Place::new().grow()).node(Flex::row().gap(14.0));
            body.place(
                    Place::new()
                        .width(Sizing::fixed(330.0))
                        .height(Sizing::grow()),
                )
                .add(|ui: Cx<'_>| {
                    let mut controls = ui
                        .node(Flex::column().padding(Sides::all(14.0)).gap(7.0))
                        .insert(panel(colors::SURFACE));
                    controls.add(
                        Text::new("LAYOUT PARAMETERS")
                            .style(TextStyle {
                                size: 12.0,
                                ..TextStyle::default()
                            })
                            .color(colors::ACCENT),
                    );
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
                            "layout",
                            &mut self.canvas.layout,
                            &[
                                ("Flex", CanvasLayout::Flex),
                                ("Wrap", CanvasLayout::Wrap),
                                ("Grid", CanvasLayout::Grid),
                            ],
                        );
                    });
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
                            "axis",
                            &mut self.canvas.axis,
                            &[("Horizontal", Axis::Horizontal), ("Vertical", Axis::Vertical)],
                        );
                    });
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
                            "justify",
                            &mut self.canvas.justify,
                            &[
                                ("Start", blit_desktop::layout::Justify::Start),
                                ("Center", blit_desktop::layout::Justify::Center),
                                ("End", blit_desktop::layout::Justify::End),
                            ],
                        );
                    });
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
                            "space",
                            &mut self.canvas.justify,
                            &[
                                ("Between", blit_desktop::layout::Justify::SpaceBetween),
                                ("Around", blit_desktop::layout::Justify::SpaceAround),
                                ("Evenly", blit_desktop::layout::Justify::SpaceEvenly),
                            ],
                        );
                    });
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
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
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
                            "sizing",
                            &mut self.canvas.sizing,
                            &[
                                ("Fixed", ItemSizing::Fixed),
                                ("Fit", ItemSizing::Fit),
                                ("Grow", ItemSizing::Grow),
                            ],
                        );
                    });
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
                            "zoom",
                            &mut self.canvas.zoom,
                            &[("75%", 0.75), ("100%", 1.0), ("125%", 1.25)],
                        );
                    });
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
                            "gap",
                            &mut self.canvas.gap_steps,
                            &[("0", 0), ("1", 1), ("2", 2), ("3", 3)],
                        );
                    });
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
                            "padding",
                            &mut self.canvas.padding_steps,
                            &[("0", 0), ("1", 1), ("2", 2), ("3", 3)],
                        );
                    });
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
                            "transition",
                            &mut self.canvas.transitions,
                            &[("On", true), ("Off", false)],
                        );
                    });
                    controls.place(Place::new().grow()).add(
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
            body.place(Place::new().grow()).add(|ui: Cx<'_>| {
                let mut preview = ui
                    .node(Flex::column().padding(Sides::all(12.0)).gap(8.0))
                    .insert(panel(colors::SURFACE));
                preview.place(Place::new().height(Sizing::fixed(22.0))).add(
                    Text::new("LIVE PREVIEW")
                        .style(TextStyle {
                            size: 11.0,
                            ..TextStyle::default()
                        })
                        .color(colors::ACCENT),
                );
                preview.place(Place::new().grow()).add(|ui: Cx<'_>| {
                    let mut viewport = ui
                        .node(Flex::row().padding(Sides::all(10.0)).align(Align::Start))
                        .insert(
                            Rectangle::new()
                                .background(colors::TRACK)
                                .radius(BorderRadius::uniform(8.0)),
                        );
                    viewport.add(
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
                            DesktopGrip,
                        )
                        .minimum(Size::new(240.0, 180.0))
                        .maximum(Size::new(
                            (screen.width - 390.0).max(240.0),
                            (screen.height - 120.0).max(180.0),
                        ))
                        .grip_size(Size::new(12.0, 12.0)),
                    );
                });
            });
        } else {
            let mut section = root
                .place(Place::new().grow())
                .node(Flex::column().padding(Sides::all(12.0)).gap(8.0))
                .insert(panel(colors::SURFACE));
            {
                let mut header = section.node(Flex::row().gap(6.0).align(Align::Center));
                header.place(Place::new().grow()).add(
                    Text::new("SCROLL AREA")
                        .style(TextStyle {
                            size: 11.0,
                            ..TextStyle::default()
                        })
                        .color(colors::TEXT_MUTED),
                );
                for (axis, label) in [
                    (Axis::Vertical, "VERTICAL"),
                    (Axis::Horizontal, "HORIZONTAL"),
                ] {
                    if header.add(Button::new(
                        WidgetId::new(("desktop scroll axis", label)),
                        label,
                        self.scroll_axis == axis,
                    )) {
                        self.scroll_axis = axis;
                        self.scroll = scroll::State::default();
                    }
                }
            }
            let axis = self.scroll_axis;
            section.place(Place::new().grow()).add(
                scroll::Area::new(&mut self.scroll, BoundsClip, move |ui: Cx<'_>| {
                    let mut items = ui.node(Flex::new(axis).padding(Sides::all(6.0)).gap(6.0));
                    for index in 0..100 {
                        let item = ITEMS[index % ITEMS.len()];
                        let place = match axis {
                            Axis::Horizontal => Place::new().width(Sizing::fixed(92.0)),
                            Axis::Vertical => Place::new().height(Sizing::fixed(30.0)),
                        };
                        items.place(place).add(|ui: Cx<'_>| {
                            let layout = match axis {
                                Axis::Horizontal => Flex::column()
                                    .align(Align::Center)
                                    .justify(blit_desktop::layout::Justify::Center),
                                Axis::Vertical => {
                                    Flex::row().padding(Sides::all(6.0)).align(Align::Center)
                                }
                            };
                            let background = if index % 2 == 0 {
                                colors::CANVAS
                            } else {
                                colors::SURFACE_HIGH
                            };
                            let mut tile = ui.node(layout).insert(
                                Rectangle::new()
                                    .background(background)
                                    .radius(BorderRadius::uniform(5.0)),
                            );
                            tile.add(
                                Text::new(item.label)
                                    .style(TextStyle {
                                        size: 10.0,
                                        ..TextStyle::default()
                                    })
                                    .color(colors::TEXT),
                            );
                        });
                    }
                    items.id()
                })
                .axis(axis)
                .scroll_track(|_| Rectangle::new().background(colors::TRACK))
                .scrollbar(|active| {
                    Rectangle::new()
                        .background(if active {
                            colors::TEXT_DIM
                        } else {
                            colors::BORDER
                        })
                        .radius(BorderRadius::uniform(3.0))
                })
                .scrollbar_thickness(6.0)
                .minimum_scrollbar_extent(24.0),
            );
        }
        root.add(&mut self.fps);
    }
}

struct FpsBadge {
    counter: FpsCounter,
    label: String,
}

impl Default for FpsBadge {
    fn default() -> Self {
        Self {
            counter: FpsCounter::default(),
            label: "FPS --".into(),
        }
    }
}

impl Widget<DesktopPlatform> for &mut FpsBadge {
    type Response = ();

    fn build(self, ui: Cx<'_>) {
        if let Some(fps) = self.counter.update(ui.time()) {
            self.label.clear();
            let _ = write!(self.label, "FPS {fps:03.0}");
        }
        let mut badge = ui
            .node(
                Flex::row()
                    .padding(Sides::new().top(6.0).right(10.0).bottom(6.0).left(10.0))
                    .gap(6.0)
                    .align(Align::Center),
            )
            .insert(
                Rectangle::new()
                    .background(colors::SURFACE_HIGH)
                    .border(Border::solid(1.0, colors::ACCENT))
                    .radius(BorderRadius::uniform(7.0)),
            )
            .absolute(
                Absolute::screen(0.0, 0.0)
                    .anchors(Anchor::BottomRight, Anchor::BottomRight)
                    .offset(-16.0, -16.0),
            );
        badge.place(Place::new().fixed(6.0, 6.0)).add(
            Rectangle::new()
                .background(colors::ACCENT)
                .radius(BorderRadius::uniform(3.0)),
        );
        badge.add(
            Text::new(&self.label)
                .style(TextStyle {
                    size: 10.0,
                    ..TextStyle::default()
                })
                .color(colors::TEXT),
        );
        badge.add(
            Text::new("SCREEN ABSOLUTE")
                .style(TextStyle {
                    size: 10.0,
                    ..TextStyle::default()
                })
                .color(colors::TEXT_DIM),
        );
    }
}

struct DesktopGrip(ResizeGrip);

impl Widget<DesktopPlatform> for DesktopGrip {
    type Response = NodeId;

    fn build(self, ui: Cx<'_>) -> NodeId {
        let marker = match self.0.edge {
            ResizeEdge::Right => Size::new(3.0, 48.0),
            ResizeEdge::Bottom => Size::new(48.0, 3.0),
            ResizeEdge::Corner => Size::new(6.0, 6.0),
        };
        let active =
            self.0.interaction.hovered || self.0.interaction.active || self.0.interaction.dragging;
        let color = if active {
            colors::ACCENT
        } else if self.0.edge == ResizeEdge::Corner {
            colors::GRIP_CORNER
        } else {
            colors::GRIP
        };
        let mut grip = ui.node(
            Flex::row()
                .align(Align::Center)
                .justify(blit_desktop::layout::Justify::Center),
        );
        let node = grip.id();
        grip.place(Place::new().fixed(marker.width, marker.height))
            .add(
                Rectangle::new()
                    .background(color)
                    .radius(BorderRadius::uniform(marker.width.min(marker.height) / 2.0)),
            );
        node
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

    fn build(self, mut ui: Cx<'_>) -> bool {
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
            .node(Flex::row().padding(Sides::new().top(5.0).right(8.0).bottom(5.0).left(8.0)))
            .insert(
                Rectangle::new()
                    .background(background)
                    .border(Border::solid(1.0, border))
                    .radius(BorderRadius::uniform(5.0)),
            )
            .widget_id(self.id);
        button.add(
            Text::new(self.label)
                .style(TextStyle {
                    size: 10.0,
                    ..TextStyle::default()
                })
                .color(colors::TEXT),
        );
        interaction.clicked
    }
}

fn choices<T: Copy + PartialEq>(ui: Cx<'_>, label: &str, selected: &mut T, options: &[(&str, T)]) {
    let mut line = ui.node(Flex::row().align(Align::Center).gap(6.0));
    line.place(Place::new().width(Sizing::fixed(66.0))).add(
        Text::new(label)
            .style(TextStyle {
                size: 10.0,
                ..TextStyle::default()
            })
            .color(colors::TEXT_MUTED),
    );
    line.place(Place::new().grow()).add(|ui: Cx<'_>| {
        let mut values = ui.node(Flex::row().gap(4.0));
        for (index, &(option, value)) in options.iter().enumerate() {
            let clicked = values.add(Button::new(
                WidgetId::new((label, index)),
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

impl Widget<DesktopPlatform> for Canvas {
    type Response = ();

    fn build(self, ui: Cx<'_>) {
        let background = Rectangle::new()
            .background(colors::CANVAS)
            .border(Border::solid(2.0, colors::CANVAS_BORDER))
            .radius(BorderRadius::uniform(8.0));
        match self.config.layout {
            CanvasLayout::Flex => {
                let mut canvas = ui
                    .node(
                        Flex::new(self.config.axis)
                            .padding(self.config.padding(self.unit))
                            .gap(self.config.gap(self.config.axis, self.unit))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .insert(background)
                    .clip(BoundsClip);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    canvas
                        .place(self.config.item_place(index, self.unit))
                        .add(|ui: Cx<'_>| {
                            canvas_item(ui, index, spec, badges, self.config);
                        });
                }
            }
            CanvasLayout::Wrap => {
                let cross = match self.config.axis {
                    Axis::Horizontal => Axis::Vertical,
                    Axis::Vertical => Axis::Horizontal,
                };
                let mut canvas = ui
                    .node(
                        Wrap::new(self.config.axis)
                            .padding(self.config.padding(self.unit))
                            .item_gap(self.config.gap(self.config.axis, self.unit))
                            .run_gap(self.config.gap(cross, self.unit))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .insert(background)
                    .clip(BoundsClip);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    canvas
                        .place(self.config.item_place(index, self.unit))
                        .add(|ui: Cx<'_>| {
                            canvas_item(ui, index, spec, badges, self.config);
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
                let mut canvas = ui.node(grid).insert(background).clip(BoundsClip);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    let item = placer.place(spec.rows, spec.columns);
                    canvas
                        .item(item)
                        .place(
                            Place::new()
                                .height(Sizing::fixed(5.0 * self.unit.height * self.config.zoom)),
                        )
                        .add(|ui: Cx<'_>| {
                            canvas_item(ui, index, spec, badges, self.config);
                        });
                }
            }
        }
    }
}

fn canvas_item(
    ui: Cx<'_>,
    index: usize,
    spec: blit_showcase::ItemSpec,
    badges: blit::LayerId,
    config: CanvasConfig,
) {
    let item = ui
        .node(
            Flex::column()
                .align(Align::Center)
                .justify(blit_desktop::layout::Justify::Center),
        )
        .insert(
            Rectangle::new()
                .background(colors::ITEMS[index])
                .radius(BorderRadius::uniform(5.0)),
        );
    let mut item = if config.transitions {
        item.widget_id(WidgetId::new(("canvas item", index)))
            .transition(
                Transition::new(Duration::from_millis(320))
                    .easing(Easing::EaseOutQuad)
                    .layout(),
            )
    } else {
        item
    };
    item.add(
        Text::new(spec.label)
            .style(TextStyle {
                size: 11.0 * config.zoom,
                ..TextStyle::default()
            })
            .color(Color::WHITE),
    );
    if let Some(anchor) = spec.badge {
        item.place(
            Place::new()
                .fixed(28.0 * config.zoom, 14.0 * config.zoom)
                .layer(badges)
                .z_index(1),
        )
        .add(|ui: Cx<'_>| {
            let mut badge = ui
                .node(
                    Flex::row()
                        .align(Align::Center)
                        .justify(blit_desktop::layout::Justify::Center),
                )
                .insert(
                    Rectangle::new()
                        .background(colors::BACKGROUND)
                        .border(Border::solid(1.0, Color::WHITE))
                        .radius(BorderRadius::uniform(4.0)),
                )
                .absolute(Absolute::attach(anchor, Anchor::Center));
            badge.add(
                Text::new("ABS")
                    .style(TextStyle {
                        size: (7.0 * config.zoom).max(7.0),
                        ..TextStyle::default()
                    })
                    .color(Color::WHITE),
            );
        });
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
