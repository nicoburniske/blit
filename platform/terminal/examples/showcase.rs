use std::{fmt::Write as _, io, time::Duration};

use blit::{
    Absolute, Anchor, Axis, Easing, Input, Key, NodeId, Place, Sense, Sides, Size, Sizing,
    Transition, Widget, WidgetId,
};
use blit_showcase::{
    CanvasConfig, CanvasLayout, FpsCounter, ITEMS, ItemSizing, Resizable, ResizeEdge, ResizeGrip,
    ResizeState,
};
use blit_terminal::{
    BoundsClip, ControlFlow, Cx, TerminalPlatform, Ui,
    atom::{Block, Border, BorderSides, BorderStyle, Shadow, Title, TitlePosition},
    color::Color,
    layout::{Align, Flex, Grid, Justify, Wrap},
    text::{
        HorizontalAlign, Span, TextAttributes, TextOptions, TextOverflow, TextWrap, VerticalAlign,
    },
    widget::Text,
};

fn main() -> io::Result<()> {
    let mut page = Page::default();
    let mut canvas = CanvasConfig::default();
    let mut resize = ResizeState::default();
    let mut text_resize = ResizeState::default();
    let mut text_attributes = TextAttributes::NONE;
    let mut text_wrap = TextWrap::Word;
    let mut text_overflow = TextOverflow::Clip;
    let mut text_horizontal = HorizontalAlign::Left;
    let mut text_vertical = VerticalAlign::Top;
    let mut text_max_lines = None;
    let mut block_style = BorderStyle::Rounded;
    let mut block_sides = BorderSides::ALL;
    let mut block_shadow = true;
    let mut block_background = true;
    let mut fps = FpsBadge::default();
    blit_terminal::run(|ui: &mut Ui| {
        let control = if matches!(ui.input(), Input::Text('q'))
            || matches!(ui.input(), Input::Key(key) if key.key == Key::Escape)
        {
            ControlFlow::Exit
        } else {
            ControlFlow::Continue
        };
        let screen = ui.screen();
        let mut root = ui
            .node(Flex::column().padding(Sides::all(1.0)).gap(1.0))
            .surface(Block::new().background(colors::BACKGROUND));
        root.child()
            .place(Place::new().height(Sizing::fixed(1.0)))
            .node(Flex::row().align(Align::Center).gap(1.0), |mut header| {
                header = header.surface(Block::new().background(colors::SURFACE));
                header.add(Text::new(" BLIT ").attributes(TextAttributes::BOLD));
                if header.add(Button::new(
                    WidgetId::new("terminal layout page"),
                    " Layout ",
                    page == Page::Layout,
                )) {
                    page = Page::Layout;
                }
                if header.add(Button::new(
                    WidgetId::new("terminal text page"),
                    " Text ",
                    page == Page::Text,
                )) {
                    page = Page::Text;
                }
                if header.add(Button::new(
                    WidgetId::new("terminal blocks page"),
                    " Blocks ",
                    page == Page::Blocks,
                )) {
                    page = Page::Blocks;
                }
                if header.add(Button::new(
                    WidgetId::new("terminal reset showcase"),
                    " Reset ",
                    false,
                )) {
                    canvas = CanvasConfig::default();
                    resize.reset();
                    text_resize.reset();
                    text_attributes = TextAttributes::NONE;
                    text_wrap = TextWrap::Word;
                    text_overflow = TextOverflow::Clip;
                    text_horizontal = HorizontalAlign::Left;
                    text_vertical = VerticalAlign::Top;
                    text_max_lines = None;
                    block_style = BorderStyle::Rounded;
                    block_sides = BorderSides::ALL;
                    block_shadow = true;
                    block_background = true;
                }
                header.add(Text::new("q quit").color(colors::TEXT_MUTED));
            });
        if page == Page::Layout {
            root.child()
                .place(Place::new().grow())
                .node(Flex::row().gap(1.0), |mut body| {
                    body.child()
                        .place(
                            Place::new()
                                .width(Sizing::fixed(40.0))
                                .height(Sizing::grow()),
                        )
                        .node(
                            Flex::column().padding(Sides::all(1.0)).gap(1.0),
                            |mut controls| {
                                controls =
                                    controls.surface(panel(colors::SURFACE, " LAYOUT PARAMETERS "));
                                controls.add(
                                    Text::new("FLOW")
                                        .color(colors::SECTION)
                                        .attributes(TextAttributes::BOLD),
                                );
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
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
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
                                        "axis",
                                        "terminal axis",
                                        &mut canvas.axis,
                                        &[(" Horz ", Axis::Horizontal), (" Vert ", Axis::Vertical)],
                                    );
                                });
                                controls.add(
                                    Text::new("DISTRIBUTION")
                                        .color(colors::SECTION)
                                        .attributes(TextAttributes::BOLD),
                                );
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
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
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
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
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
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
                                        .color(colors::SECTION)
                                        .attributes(TextAttributes::BOLD),
                                );
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
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
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
                                        "zoom",
                                        "terminal zoom",
                                        &mut canvas.zoom,
                                        &[(" 75% ", 0.75), (" 100% ", 1.0), (" 125% ", 1.25)],
                                    );
                                });
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
                                        "gap",
                                        "terminal gap",
                                        &mut canvas.gap_steps,
                                        &[(" 0 ", 0), (" 1 ", 1), (" 2 ", 2), (" 3 ", 3)],
                                    );
                                });
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
                                        "padding",
                                        "terminal padding",
                                        &mut canvas.padding_steps,
                                        &[(" 0 ", 0), (" 1 ", 1), (" 2 ", 2), (" 3 ", 3)],
                                    );
                                });
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
                                        "transitions",
                                        "terminal transitions",
                                        &mut canvas.transitions,
                                        &[(" On ", true), (" Off ", false)],
                                    );
                                });
                                controls.child().place(Place::new().grow()).node(
                                    Flex::column().justify(Justify::End),
                                    |mut help| {
                                        help.add(
                                            Text::new("POINTER")
                                                .color(colors::SECTION)
                                                .attributes(TextAttributes::BOLD),
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
                    body.child().place(Place::new().grow()).node(
                        Flex::column().padding(Sides::all(1.0)),
                        |mut preview| {
                            preview = preview.surface(
                                panel(colors::SURFACE, " LIVE PREVIEW ").title(
                                    Title::new(match (canvas.layout, canvas.axis) {
                                        (CanvasLayout::Flex, Axis::Horizontal) => {
                                            " FLEX / HORIZONTAL "
                                        }
                                        (CanvasLayout::Flex, Axis::Vertical) => " FLEX / VERTICAL ",
                                        (CanvasLayout::Wrap, Axis::Horizontal) => {
                                            " WRAP / HORIZONTAL "
                                        }
                                        (CanvasLayout::Wrap, Axis::Vertical) => " WRAP / VERTICAL ",
                                        (CanvasLayout::Grid, Axis::Horizontal) => {
                                            " GRID / HORIZONTAL "
                                        }
                                        (CanvasLayout::Grid, Axis::Vertical) => " GRID / VERTICAL ",
                                    })
                                    .color(colors::TEXT_MUTED)
                                    .position(TitlePosition::TopRight),
                                ),
                            );
                            preview.child().place(Place::new().grow()).node(
                                Flex::row().padding(Sides::all(1.0)).align(Align::Start),
                                |mut viewport| {
                                    viewport =
                                        viewport.surface(Block::new().background(colors::TRACK));
                                    viewport.add(
                                        Resizable::new(
                                            &mut resize,
                                            WidgetId::new("terminal layout canvas"),
                                            Size::new(
                                                ((screen.width - 48.0) * 0.8).max(18.0),
                                                ((screen.height - 8.0) * 0.72).max(9.0),
                                            ),
                                            Canvas { config: canvas },
                                            TerminalGrip,
                                        )
                                        .minimum(Size::new(18.0, 9.0))
                                        .maximum(screen.size())
                                        .grip_size(Size::new(1.0, 1.0)),
                                    );
                                },
                            );
                        },
                    );
                });
        } else if page == Page::Text {
            root.child()
                .place(Place::new().grow())
                .node(Flex::column().gap(1.0), |mut body| {
                    body.add(|ui: Cx<'_>| {
                        let mut controls = ui
                            .node(Flex::column().padding(Sides::all(1.0)).gap(1.0))
                            .surface(panel(colors::SURFACE, " TEXT CONTROLS "));
                        controls.add(
                            Text::new("TEXT ATTRIBUTES")
                                .color(colors::ACCENT)
                                .attributes(TextAttributes::BOLD),
                        );
                        controls.add(|ui: Cx<'_>| {
                            let mut toggles = ui.node(
                                Wrap::new(Axis::Horizontal)
                                    .item_gap(1.0)
                                    .run_gap(1.0)
                                    .align(Align::Center),
                            );
                            for (index, (label, attribute)) in [
                                (" Bold ", TextAttributes::BOLD),
                                (" Dim ", TextAttributes::DIM),
                                (" Italic ", TextAttributes::ITALIC),
                                (" Underline ", TextAttributes::UNDERLINE),
                                (" Blink ", TextAttributes::BLINK),
                                (" Inverse ", TextAttributes::INVERSE),
                                (" Hidden ", TextAttributes::HIDDEN),
                                (" Strikethrough ", TextAttributes::STRIKETHROUGH),
                            ]
                            .into_iter()
                            .enumerate()
                            {
                                let selected = text_attributes.contains(attribute);
                                if toggles.add(Button::new(
                                    WidgetId::new(("terminal text attribute", index)),
                                    label,
                                    selected,
                                )) {
                                    text_attributes.set(attribute, !selected);
                                }
                            }
                        });
                        controls.add(
                            Text::new("TEXT OPTIONS")
                                .color(colors::SECTION)
                                .attributes(TextAttributes::BOLD),
                        );
                        controls.add(|ui: Cx<'_>| {
                            choices(
                                ui,
                                "wrap",
                                "terminal text wrap",
                                &mut text_wrap,
                                &[
                                    (" None ", TextWrap::None),
                                    (" Word ", TextWrap::Word),
                                    (" Character ", TextWrap::Character),
                                ],
                            );
                        });
                        controls.add(|ui: Cx<'_>| {
                            choices(
                                ui,
                                "overflow",
                                "terminal text overflow",
                                &mut text_overflow,
                                &[
                                    (" Clip ", TextOverflow::Clip),
                                    (" Ellipsis ", TextOverflow::Ellipsis),
                                ],
                            );
                        });
                        controls.add(|ui: Cx<'_>| {
                            choices(
                                ui,
                                "horizontal",
                                "terminal text horizontal align",
                                &mut text_horizontal,
                                &[
                                    (" Left ", HorizontalAlign::Left),
                                    (" Center ", HorizontalAlign::Center),
                                    (" Right ", HorizontalAlign::Right),
                                ],
                            );
                        });
                        controls.add(|ui: Cx<'_>| {
                            choices(
                                ui,
                                "vertical",
                                "terminal text vertical align",
                                &mut text_vertical,
                                &[
                                    (" Top ", VerticalAlign::Top),
                                    (" Center ", VerticalAlign::Center),
                                    (" Bottom ", VerticalAlign::Bottom),
                                ],
                            );
                        });
                        controls.add(|ui: Cx<'_>| {
                            choices(
                                ui,
                                "maximum lines",
                                "terminal text maximum lines",
                                &mut text_max_lines,
                                &[(" All ", None), (" 3 ", Some(3)), (" 6 ", Some(6))],
                            );
                        });
                    });
                    body.child().place(Place::new().grow()).node(
                        Flex::column().padding(Sides::all(1.0)).gap(1.0),
                        |mut preview| {
                            preview = preview.surface(panel(colors::SURFACE, " RESIZABLE TEXT ").title(
                                Title::new(" DRAG TO REFLOW ")
                                    .color(colors::TEXT_DIM)
                                    .position(TitlePosition::BottomRight),
                            ));
                            preview.child().place(Place::new().grow()).node(
                                Flex::row().padding(Sides::all(1.0)).align(Align::Start),
                                |mut viewport| {
                                    viewport = viewport.surface(Block::new().background(colors::TRACK));
                                    let mut options = TextOptions::new()
                                        .wrap(text_wrap)
                                        .overflow(text_overflow)
                                        .horizontal_align(text_horizontal)
                                        .vertical_align(text_vertical);
                                    if let Some(max_lines) = text_max_lines {
                                        options = options.max_lines(max_lines);
                                    }
                                    viewport.add(
                                        Resizable::new(
                                            &mut text_resize,
                                            WidgetId::new("terminal text preview"),
                                            Size::new(
                                                (screen.width * 0.7).max(12.0),
                                                (screen.height * 0.35).max(6.0),
                                            ),
                                            |ui: Cx<'_>| {
                                                let mut paragraph = ui
                                                    .node(
                                                        Flex::column().padding(Sides::all(1.0)),
                                                    )
                                                    .surface(
                                                        Block::new()
                                                            .background(colors::CANVAS)
                                                            .border(
                                                                Border::new(colors::CANVAS_BORDER)
                                                                    .style(BorderStyle::Rounded),
                                                            ),
                                                    )
                                                    .clip(BoundsClip);
                                                let sample = [
                                                    Span::new("The terminal renderer lays out "),
                                                    Span::new("rich text")
                                                        .color(colors::ACCENT)
                                                        .attributes(TextAttributes::BOLD),
                                                    Span::new(
                                                        " as one paragraph. Style boundaries do not change wrapping, alignment, or measurement. Toggle the controls to combine attributes across every span while ",
                                                    ),
                                                    Span::new("individual spans")
                                                        .color(colors::SECTION)
                                                        .attributes(TextAttributes::UNDERLINE),
                                                    Span::new(
                                                        " retain their own emphasis and the words continue flowing through the same layout.",
                                                    ),
                                                ];
                                                paragraph.child().place(Place::new().grow()).add(
                                                    Text::rich(&sample)
                                                        .color(colors::TEXT)
                                                        .attributes(text_attributes)
                                                        .options(options),
                                                );
                                            },
                                            TerminalGrip,
                                        )
                                        .minimum(Size::new(12.0, 6.0))
                                        .maximum(Size::new(
                                            (screen.width - 4.0).max(12.0),
                                            (screen.height - 8.0).max(6.0),
                                        ))
                                        .grip_size(Size::new(1.0, 1.0)),
                                    );
                                },
                            );
                        },
                    );
                });
        } else {
            root.child()
                .place(Place::new().grow())
                .node(Flex::row().gap(1.0), |mut body| {
                    body.child()
                        .place(
                            Place::new()
                                .width(Sizing::fixed(40.0))
                                .height(Sizing::grow()),
                        )
                        .node(
                            Flex::column().padding(Sides::all(1.0)).gap(1.0),
                            |mut controls| {
                                controls =
                                    controls.surface(panel(colors::SURFACE, " BLOCK OPTIONS "));
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
                                        "border style",
                                        "terminal block border style",
                                        &mut block_style,
                                        &[
                                            (" Single ", BorderStyle::Single),
                                            (" Rounded ", BorderStyle::Rounded),
                                            (" Double ", BorderStyle::Double),
                                            (" Heavy ", BorderStyle::Heavy),
                                        ],
                                    );
                                });
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
                                        "border sides",
                                        "terminal block border sides",
                                        &mut block_sides,
                                        &[
                                            (" All ", BorderSides::ALL),
                                            (
                                                " Horizontal ",
                                                BorderSides::TOP | BorderSides::BOTTOM,
                                            ),
                                            (" Vertical ", BorderSides::LEFT | BorderSides::RIGHT),
                                            (" None ", BorderSides::NONE),
                                        ],
                                    );
                                });
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
                                        "shadow",
                                        "terminal block shadow",
                                        &mut block_shadow,
                                        &[(" On ", true), (" Off ", false)],
                                    );
                                });
                                controls.add(|ui: Cx<'_>| {
                                    choices(
                                        ui,
                                        "background",
                                        "terminal block background",
                                        &mut block_background,
                                        &[(" On ", true), (" Off ", false)],
                                    );
                                });
                            },
                        );
                    body.child().place(Place::new().grow()).node(
                        Flex::column().padding(Sides::all(1.0)).gap(1.0),
                        |mut preview| {
                            preview = preview.surface(panel(colors::SURFACE, " BLOCK PREVIEW "));
                            preview
                                .child()
                                .place(Place::new().grow())
                                .add(|ui: Cx<'_>| {
                                    let mut block = Block::new()
                                        .border(
                                            Border::new(colors::CANVAS_BORDER)
                                                .style(block_style)
                                                .sides(block_sides),
                                        )
                                        .title(
                                            Title::new(" CONFIGURED BLOCK ")
                                                .color(colors::ACCENT)
                                                .attributes(TextAttributes::BOLD),
                                        );
                                    if block_background {
                                        block = block.background(colors::SURFACE_HIGH);
                                    }
                                    if block_shadow {
                                        block = block.shadow(Shadow::new(colors::SHADOW));
                                    }
                                    let mut configured = ui
                                        .node(
                                            Flex::column()
                                                .padding(Sides::all(1.0))
                                                .align(Align::Center)
                                                .justify(Justify::Center),
                                        )
                                        .surface(block);
                                    configured.add(
                                        Text::new("change the options on the left")
                                            .color(colors::TEXT_MUTED),
                                    );
                                });
                        },
                    );
                });
        }
        root.add(&mut fps);
        control
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Layout,
    Text,
    Blocks,
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

impl Widget<TerminalPlatform> for &mut FpsBadge {
    type Response = ();

    fn build(self, ui: Cx<'_>) {
        if let Some(fps) = self.counter.update(ui.time()) {
            self.label.clear();
            let _ = write!(self.label, "FPS {fps:03.0}");
        }
        let mut badge = ui
            .node(
                Flex::row()
                    .padding(Sides::all(1.0))
                    .gap(1.0)
                    .align(Align::Center),
            )
            .surface(
                Block::new()
                    .background(colors::SURFACE_HIGH)
                    .border(Border::new(colors::ACCENT).style(BorderStyle::Rounded)),
            )
            .absolute(
                Absolute::screen(0.0, 0.0)
                    .anchors(Anchor::BottomRight, Anchor::BottomRight)
                    .offset(-1.0, -1.0),
            );
        badge
            .child()
            .place(Place::new().fixed(1.0, 1.0))
            .add(Block::new().background(colors::ACCENT));
        badge.add(
            Text::new(&self.label)
                .color(colors::TEXT)
                .attributes(TextAttributes::BOLD),
        );
        badge.add(Text::new("SCREEN ABSOLUTE").color(colors::TEXT_DIM));
    }
}

struct TerminalGrip(ResizeGrip);

impl Widget<TerminalPlatform> for TerminalGrip {
    type Response = NodeId;

    fn build(self, ui: Cx<'_>) -> NodeId {
        let marker = match self.0.edge {
            ResizeEdge::Right => Size::new(1.0, 3.0),
            ResizeEdge::Bottom => Size::new(5.0, 1.0),
            ResizeEdge::Corner => Size::new(1.0, 1.0),
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
        let mut grip = ui.node(Flex::row().align(Align::Center).justify(Justify::Center));
        let node = grip.id();
        grip.child()
            .place(Place::new().fixed(marker.width, marker.height))
            .add(Block::new().background(color));
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

impl Widget<TerminalPlatform> for Button<'_> {
    type Response = bool;

    fn build(self, mut ui: Cx<'_>) -> bool {
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
        let mut button = ui.node(Flex::row()).surface(block).widget_id(self.id);
        button.add(Text::new(self.label).color(colors::TEXT));
        interaction.clicked
    }
}

fn choices<T: Copy + PartialEq>(
    ui: Cx<'_>,
    label: &str,
    id: &str,
    selected: &mut T,
    options: &[(&str, T)],
) {
    let mut group = ui.node(Flex::column());
    group.add(Text::new(label).color(colors::TEXT_MUTED));
    group.add(|ui: Cx<'_>| {
        let mut values = ui.node(
            Wrap::new(Axis::Horizontal)
                .item_gap(2.0)
                .run_gap(1.0)
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
}

impl Widget<TerminalPlatform> for Canvas {
    type Response = ();

    fn build(self, ui: Cx<'_>) {
        let unit = Size::new(1.0, 1.0);
        let background = Block::new()
            .background(colors::CANVAS)
            .border(Border::new(colors::CANVAS_BORDER).style(BorderStyle::Rounded));
        match self.config.layout {
            CanvasLayout::Flex => {
                let mut canvas = ui
                    .node(
                        Flex::new(self.config.axis)
                            .padding(self.config.padding(unit))
                            .gap(self.config.gap(self.config.axis, unit))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .surface(background)
                    .clip(BoundsClip);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    canvas
                        .child()
                        .place(self.config.item_place(index, unit))
                        .add(|ui: Cx<'_>| {
                            canvas_item(ui, index, spec, badges, self.config, unit);
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
                            .padding(self.config.padding(unit))
                            .item_gap(self.config.gap(self.config.axis, unit))
                            .run_gap(self.config.gap(cross, unit))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .surface(background)
                    .clip(BoundsClip);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    canvas
                        .child()
                        .place(self.config.item_place(index, unit))
                        .add(|ui: Cx<'_>| {
                            canvas_item(ui, index, spec, badges, self.config, unit);
                        });
                }
            }
            CanvasLayout::Grid => {
                let grid = Grid::columns(5)
                    .spanning()
                    .padding(self.config.padding(unit))
                    .column_gap(self.config.gap(Axis::Horizontal, unit))
                    .row_gap(self.config.gap(Axis::Vertical, unit));
                let mut placer = grid.placer();
                let mut canvas = ui.node(grid).surface(background).clip(BoundsClip);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    let item = placer.place(spec.rows, spec.columns);
                    canvas
                        .child()
                        .item(item)
                        .place(Place::new().height(Sizing::fixed(3.0 * self.config.zoom)))
                        .add(|ui: Cx<'_>| {
                            canvas_item(ui, index, spec, badges, self.config, unit);
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
    unit: Size,
) {
    let item = ui
        .node(Flex::column().align(Align::Center).justify(Justify::Center))
        .surface(Block::new().background(colors::ITEMS[index]));
    let mut item = if config.transitions {
        item.widget_id(WidgetId::new(("terminal canvas item", index)))
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
            .color(colors::TEXT)
            .attributes(TextAttributes::BOLD),
    );
    if let Some(anchor) = spec.badge {
        item.child()
            .place(
                Place::new()
                    .fixed(unit.width * 2.0, unit.height)
                    .layer(badges)
                    .z_index(1),
            )
            .add(|ui: Cx<'_>| {
                let mut badge = ui
                    .node(Flex::row().align(Align::Center).justify(Justify::Center))
                    .surface(Block::new().background(colors::ACCENT_DARK))
                    .absolute(Absolute::attach(anchor, Anchor::Center));
                badge.add(Text::new("A").color(colors::TEXT));
            });
    }
}

fn panel<'a>(background: Color, title: &'a str) -> Block<'a> {
    Block::new()
        .background(background)
        .border(Border::new(colors::BORDER).style(BorderStyle::Rounded))
        .title(
            Title::new(title)
                .color(colors::ACCENT)
                .attributes(TextAttributes::BOLD),
        )
}

mod colors {
    use blit_terminal::color::Color;

    pub const BACKGROUND: Color = Color::Reset;
    pub const SURFACE: Color = Color::BLACK;
    pub const SURFACE_HIGH: Color = Color::DARK_GRAY;
    pub const TRACK: Color = Color::BLACK;
    pub const SELECTED: Color = Color::BLUE;
    pub const CANVAS: Color = Color::BLACK;
    pub const CANVAS_BORDER: Color = Color::GRAY;
    pub const GRIP: Color = Color::CYAN;
    pub const GRIP_CORNER: Color = Color::LIGHT_CYAN;
    pub const BORDER: Color = Color::DARK_GRAY;
    pub const SHADOW: Color = Color::DARK_GRAY;
    pub const TEXT: Color = Color::WHITE;
    pub const TEXT_MUTED: Color = Color::GRAY;
    pub const TEXT_DIM: Color = Color::DARK_GRAY;
    pub const SECTION: Color = Color::LIGHT_BLUE;
    pub const ACCENT: Color = Color::LIGHT_CYAN;
    pub const ACCENT_DARK: Color = Color::CYAN;
    pub const ITEMS: [Color; 10] = [
        Color::BLUE,
        Color::GREEN,
        Color::YELLOW,
        Color::MAGENTA,
        Color::RED,
        Color::CYAN,
        Color::LIGHT_BLUE,
        Color::LIGHT_GREEN,
        Color::LIGHT_MAGENTA,
        Color::LIGHT_RED,
    ];
}
