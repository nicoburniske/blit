use std::{fmt::Write as _, io, time::Duration};

use blit::{
    Absolute, Anchor, Axis, Easing, Input, Key, NodeId, Place, Sense, Sides, Size, Sizing,
    Transition, Widget, WidgetId,
};
use blit_showcase::{
    CanvasConfig, CanvasLayout, FpsCounter, ITEMS, ItemSizing, Resizable, ResizeEdge, ResizeGrip,
    ResizeState,
};
use blit_tui::{
    BoundsClip, ControlFlow, Cx, TuiPlatform, Ui,
    atom::{Border, BorderSides, BorderStyle, Shadow, TitlePosition},
    color::Color,
    layout::{Align, Flex, Grid, Justify, Wrap},
    text::{
        HorizontalAlign, Span, TextAttributes, TextOptions, TextOverflow, TextWrap, VerticalAlign,
    },
    widget::{Block, Text, Title, scroll},
};

fn main() -> io::Result<()> {
    let mut showcase = Showcase::default();
    blit_tui::run(|ui| showcase.show(ui))
}

#[derive(Default)]
struct Showcase {
    page: Page,
    layout: LayoutPage,
    text: TextPage,
    blocks: BlocksPage,
    scroll: ScrollPage,
    fps: FpsBadge,
}

impl Showcase {
    fn show(&mut self, ui: &mut Ui) -> ControlFlow {
        let control = if matches!(ui.input(), Input::Text('q'))
            || matches!(ui.input(), Input::Key(key) if key.key == Key::Escape)
        {
            ControlFlow::Exit
        } else {
            ControlFlow::Continue
        };
        let mut root = ui
            .node(Flex::column().padding(Sides::all(1.0)).gap(1.0))
            .insert(Block::new().background(colors::BACKGROUND));
        {
            let mut header = root
                .place(Place::new().height(Sizing::fixed(1.0)))
                .node(Flex::row().align(Align::Center).gap(1.0))
                .insert(Block::new().background(colors::SURFACE));
            header.add(Text::new(" BLIT ").attributes(TextAttributes::BOLD));
            for (page, label) in [
                (Page::Layout, " Layout "),
                (Page::Text, " Text "),
                (Page::Blocks, " Blocks "),
                (Page::Scroll, " Scroll "),
            ] {
                if header.add(Button::new(
                    WidgetId::new(("tui page", label)),
                    label,
                    self.page == page,
                )) {
                    self.page = page;
                }
            }
            if header.add(Button::new(
                WidgetId::new("tui reset showcase"),
                " Reset ",
                false,
            )) {
                self.layout = LayoutPage::default();
                self.text = TextPage::default();
                self.blocks = BlocksPage::default();
                self.scroll = ScrollPage::default();
            }
            header.add(Text::new("q quit").color(colors::TEXT_MUTED));
        }
        match self.page {
            Page::Layout => root.place(Place::new().grow()).add(&mut self.layout),
            Page::Text => root.place(Place::new().grow()).add(&mut self.text),
            Page::Blocks => root.place(Place::new().grow()).add(&mut self.blocks),
            Page::Scroll => root.place(Place::new().grow()).add(&mut self.scroll),
        }
        root.add(&mut self.fps);
        control
    }
}

#[derive(Default)]
struct LayoutPage {
    canvas: CanvasConfig,
    resize: ResizeState,
    scroll: scroll::State,
}

impl Widget<TuiPlatform> for &mut LayoutPage {
    type Response = ();

    fn build(self, ui: Cx<'_>) {
        let LayoutPage {
            canvas,
            resize,
            scroll: layout_scroll,
        } = self;
        let screen = ui.screen();
        let mut body = ui.node(Flex::row().gap(1.0));
        {
            let mut sidebar = body
                .place(
                    Place::new()
                        .width(Sizing::fixed(40.0))
                        .height(Sizing::grow()),
                )
                .node(Flex::column().padding(Sides::all(1.0)))
                .insert(panel(colors::SURFACE, " LAYOUT PARAMETERS "));
            sidebar.place(Place::new().grow()).add(
                scroll::Area::new(layout_scroll, BoundsClip, |ui: Cx<'_>| {
                    let mut controls = ui.node(Flex::column().gap(1.0));
                    controls.add(
                        Text::new("FLOW")
                            .color(colors::SECTION)
                            .attributes(TextAttributes::BOLD),
                    );
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
                            "layout",
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
                            &mut canvas.zoom,
                            &[(" 75% ", 0.75), (" 100% ", 1.0), (" 125% ", 1.25)],
                        );
                    });
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
                            "gap",
                            &mut canvas.gap_steps,
                            &[(" 0 ", 0), (" 1 ", 1), (" 2 ", 2), (" 3 ", 3)],
                        );
                    });
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
                            "padding",
                            &mut canvas.padding_steps,
                            &[(" 0 ", 0), (" 1 ", 1), (" 2 ", 2), (" 3 ", 3)],
                        );
                    });
                    controls.add(|ui: Cx<'_>| {
                        choices(
                            ui,
                            "transitions",
                            &mut canvas.transitions,
                            &[(" On ", true), (" Off ", false)],
                        );
                    });
                    controls.id()
                })
                .scroll_track(|_| Block::new().background(colors::TRACK))
                .scrollbar(|active| {
                    Block::new().background(if active {
                        colors::CANVAS_BORDER
                    } else {
                        colors::BORDER
                    })
                }),
            );
        }
        {
            let mut preview = body
                .place(Place::new().grow())
                .node(Flex::column().padding(Sides::all(1.0)))
                .insert(
                    panel(colors::SURFACE, " LIVE PREVIEW ").title(
                        Title::new(match (canvas.layout, canvas.axis) {
                            (CanvasLayout::Flex, Axis::Horizontal) => " FLEX / HORIZONTAL ",
                            (CanvasLayout::Flex, Axis::Vertical) => " FLEX / VERTICAL ",
                            (CanvasLayout::Wrap, Axis::Horizontal) => " WRAP / HORIZONTAL ",
                            (CanvasLayout::Wrap, Axis::Vertical) => " WRAP / VERTICAL ",
                            (CanvasLayout::Grid, Axis::Horizontal) => " GRID / HORIZONTAL ",
                            (CanvasLayout::Grid, Axis::Vertical) => " GRID / VERTICAL ",
                        })
                        .color(colors::TEXT_MUTED)
                        .position(TitlePosition::TopRight),
                    ),
                );
            {
                let mut viewport = preview
                    .place(Place::new().grow())
                    .node(Flex::row().padding(Sides::all(1.0)).align(Align::Start))
                    .insert(Block::new().background(colors::TRACK));
                viewport.add(
                    Resizable::new(
                        resize,
                        WidgetId::new("tui layout canvas"),
                        Size::new(
                            ((screen.width - 48.0) * 0.8).max(18.0),
                            ((screen.height - 8.0) * 0.72).max(9.0),
                        ),
                        Canvas { config: *canvas },
                        TuiGrip,
                    )
                    .minimum(Size::new(18.0, 9.0))
                    .maximum(screen.size())
                    .grip_size(Size::new(1.0, 1.0)),
                );
            }
        }
    }
}

struct TextPage {
    resize: ResizeState,
    attributes: TextAttributes,
    wrap: TextWrap,
    overflow: TextOverflow,
    horizontal: HorizontalAlign,
    vertical: VerticalAlign,
    max_lines: Option<u16>,
}

impl Default for TextPage {
    fn default() -> Self {
        Self {
            resize: ResizeState::default(),
            attributes: TextAttributes::NONE,
            wrap: TextWrap::Word,
            overflow: TextOverflow::Clip,
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
            max_lines: None,
        }
    }
}

impl Widget<TuiPlatform> for &mut TextPage {
    type Response = ();

    fn build(self, ui: Cx<'_>) {
        let TextPage {
            resize: text_resize,
            attributes: text_attributes,
            wrap: text_wrap,
            overflow: text_overflow,
            horizontal: text_horizontal,
            vertical: text_vertical,
            max_lines: text_max_lines,
        } = self;
        let screen = ui.screen();
        let mut body = ui.node(Flex::row().gap(1.0));
        body.place(
            Place::new()
                .width(Sizing::fixed(40.0))
                .height(Sizing::grow()),
        )
        .add(|ui: Cx<'_>| {
            let mut controls = ui
                .node(Flex::column().padding(Sides::all(1.0)).gap(1.0))
                .insert(panel(colors::SURFACE, " TEXT CONTROLS "));
            controls.add(
                Text::new("ATTRIBUTES")
                    .color(colors::SECTION)
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
                        WidgetId::new(("tui text attribute", index)),
                        label,
                        selected,
                    )) {
                        text_attributes.set(attribute, !selected);
                    }
                }
            });
            controls.add(
                Text::new("OPTIONS")
                    .color(colors::SECTION)
                    .attributes(TextAttributes::BOLD),
            );
            controls.add(|ui: Cx<'_>| {
                choices(
                    ui,
                    "wrap",
                    text_wrap,
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
                    text_overflow,
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
                    text_horizontal,
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
                    text_vertical,
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
                    text_max_lines,
                    &[(" All ", None), (" 3 ", Some(3)), (" 6 ", Some(6))],
                );
            });
        });
        let mut preview = body
            .place(Place::new().grow())
            .node(Flex::column().padding(Sides::all(1.0)).gap(1.0))
            .insert(
                panel(colors::SURFACE, " RESIZABLE TEXT ").title(
                    Title::new(" DRAG TO REFLOW ")
                        .color(colors::TEXT_DIM)
                        .position(TitlePosition::BottomRight),
                ),
            );
        {
            let mut viewport = preview
                .place(Place::new().grow())
                .node(Flex::row().padding(Sides::all(1.0)).align(Align::Start))
                .insert(Block::new().background(colors::TRACK));
            let mut options = TextOptions::new()
                .wrap(*text_wrap)
                .overflow(*text_overflow)
                .horizontal_align(*text_horizontal)
                .vertical_align(*text_vertical);
            if let Some(max_lines) = *text_max_lines {
                options = options.max_lines(max_lines);
            }
            viewport.add(
                Resizable::new(
                    text_resize,
                    WidgetId::new("tui text preview"),
                    Size::new(64.0, 16.0),
                    |ui: Cx<'_>| {
                        let mut paragraph = ui
                            .node(Flex::column().padding(Sides::all(1.0)))
                            .insert(
                                Block::new().background(colors::CANVAS).border(
                                    Border::new(colors::CANVAS_BORDER)
                                        .style(BorderStyle::Rounded),
                                ),
                            )
                            .clip(BoundsClip);
                        let sample = [
                            Span::new("The TUI renderer lays out "),
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
                        paragraph.place(Place::new().grow()).add(
                            Text::rich(&sample)
                                .color(colors::TEXT)
                                .attributes(*text_attributes)
                                .options(options),
                        );
                    },
                    TuiGrip,
                )
                .minimum(Size::new(12.0, 6.0))
                .maximum(screen.size())
                .grip_size(Size::new(1.0, 1.0)),
            );
        }
    }
}

struct BlocksPage {
    style: BorderStyle,
    sides: BorderSides,
    shadow: bool,
    background: bool,
}

impl Default for BlocksPage {
    fn default() -> Self {
        Self {
            style: BorderStyle::Rounded,
            sides: BorderSides::ALL,
            shadow: true,
            background: true,
        }
    }
}

impl Widget<TuiPlatform> for &mut BlocksPage {
    type Response = ();

    fn build(self, ui: Cx<'_>) {
        let BlocksPage {
            style: block_style,
            sides: block_sides,
            shadow: block_shadow,
            background: block_background,
        } = self;
        let mut body = ui.node(Flex::row().gap(1.0));
        {
            let mut controls = body
                .place(
                    Place::new()
                        .width(Sizing::fixed(40.0))
                        .height(Sizing::grow()),
                )
                .node(Flex::column().padding(Sides::all(1.0)).gap(1.0))
                .insert(panel(colors::SURFACE, " BLOCK OPTIONS "));
            controls.add(|ui: Cx<'_>| {
                choices(
                    ui,
                    "border style",
                    block_style,
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
                    block_sides,
                    &[
                        (" All ", BorderSides::ALL),
                        (" Horizontal ", BorderSides::TOP | BorderSides::BOTTOM),
                        (" Vertical ", BorderSides::LEFT | BorderSides::RIGHT),
                        (" None ", BorderSides::NONE),
                    ],
                );
            });
            controls.add(|ui: Cx<'_>| {
                choices(
                    ui,
                    "shadow",
                    block_shadow,
                    &[(" On ", true), (" Off ", false)],
                );
            });
            controls.add(|ui: Cx<'_>| {
                choices(
                    ui,
                    "background",
                    block_background,
                    &[(" On ", true), (" Off ", false)],
                );
            });
        }
        let mut preview = body
            .place(Place::new().grow())
            .node(Flex::column().padding(Sides::all(1.0)).gap(1.0))
            .insert(panel(colors::SURFACE, " BLOCK PREVIEW "));
        preview.place(Place::new().grow()).add(|ui: Cx<'_>| {
            let mut block = Block::new()
                .border(
                    Border::new(colors::CANVAS_BORDER)
                        .style(*block_style)
                        .sides(*block_sides),
                )
                .title(
                    Title::new(" CONFIGURED BLOCK ")
                        .color(colors::ACCENT)
                        .attributes(TextAttributes::BOLD),
                );
            if *block_background {
                block = block.background(colors::SURFACE_HIGH);
            }
            if *block_shadow {
                block = block.shadow(Shadow::new(colors::SHADOW));
            }
            let mut configured = ui
                .node(
                    Flex::column()
                        .padding(Sides::all(1.0))
                        .align(Align::Center)
                        .justify(Justify::Center),
                )
                .insert(block);
            configured.add(Text::new("change the options on the left").color(colors::TEXT_MUTED));
        });
    }
}

#[derive(Default)]
struct ScrollPage {
    axis: Axis,
    state: scroll::State,
}

impl Widget<TuiPlatform> for &mut ScrollPage {
    type Response = ();

    fn build(self, ui: Cx<'_>) {
        let ScrollPage {
            axis: scroll_axis,
            state: scroll,
        } = self;
        let mut body = ui
            .node(Flex::column().padding(Sides::all(1.0)).gap(1.0))
            .insert(panel(colors::SURFACE, " SCROLL AREA "));
        {
            let mut controls = body.node(Flex::row().gap(1.0).align(Align::Center));
            controls.add(
                Text::new("AXIS")
                    .color(colors::TEXT_MUTED)
                    .attributes(TextAttributes::BOLD),
            );
            for (axis, label) in [
                (Axis::Vertical, " Vertical "),
                (Axis::Horizontal, " Horizontal "),
            ] {
                if controls.add(Button::new(
                    WidgetId::new(("tui scroll axis", label)),
                    label,
                    *scroll_axis == axis,
                )) {
                    *scroll_axis = axis;
                    *scroll = scroll::State::default();
                }
            }
        }
        let axis = *scroll_axis;
        body.place(Place::new().grow()).add(
            scroll::Area::new(scroll, BoundsClip, move |ui: Cx<'_>| {
                let mut items = ui.node(Flex::new(axis).gap(1.0));
                for index in 0..100 {
                    let item = ITEMS[index % ITEMS.len()];
                    let place = match axis {
                        Axis::Horizontal => Place::new().width(Sizing::fixed(12.0)),
                        Axis::Vertical => Place::new().height(Sizing::fixed(3.0)),
                    };
                    items.place(place).add(|ui: Cx<'_>| {
                        let layout = match axis {
                            Axis::Horizontal => {
                                Flex::column().align(Align::Center).justify(Justify::Center)
                            }
                            Axis::Vertical => {
                                Flex::row().padding(Sides::all(1.0)).align(Align::Center)
                            }
                        };
                        let background = if index % 2 == 0 {
                            colors::SURFACE_HIGH
                        } else {
                            colors::TRACK
                        };
                        let mut tile = ui.node(layout).insert(
                            Block::new()
                                .background(background)
                                .border(Border::new(colors::CANVAS_BORDER)),
                        );
                        tile.add(
                            Text::new(item.label)
                                .color(colors::TEXT)
                                .attributes(TextAttributes::BOLD),
                        );
                    });
                }
                items.id()
            })
            .axis(axis)
            .scroll_track(|_| Block::new().background(colors::TRACK))
            .scrollbar(|active| {
                Block::new().background(if active {
                    colors::CANVAS_BORDER
                } else {
                    colors::BORDER
                })
            })
            .scrollbar_thickness(1.0)
            .minimum_scrollbar_extent(4.0),
        );
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Layout,
    Text,
    Blocks,
    Scroll,
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

impl Widget<TuiPlatform> for &mut FpsBadge {
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
            .insert(
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

struct TuiGrip(ResizeGrip);

impl Widget<TuiPlatform> for TuiGrip {
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
        grip.place(Place::new().fixed(marker.width, marker.height))
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

impl Widget<TuiPlatform> for Button<'_> {
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
        let mut button = ui
            .node(Flex::row().padding(Sides::x(1.0)))
            .insert(block)
            .widget_id(self.id);
        button.add(Text::new(self.label).color(colors::TEXT));
        interaction.clicked
    }
}

fn choices<T: Copy + PartialEq>(ui: Cx<'_>, label: &str, selected: &mut T, options: &[(&str, T)]) {
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
}

impl Widget<TuiPlatform> for Canvas {
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
                    .insert(background)
                    .clip(BoundsClip);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    canvas
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
                    .insert(background)
                    .clip(BoundsClip);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    canvas
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
                let mut canvas = ui.node(grid).insert(background).clip(BoundsClip);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    let item = placer.place(spec.rows, spec.columns);
                    canvas
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
        .insert(Block::new().background(colors::ITEMS[index]));
    let mut item = if config.transitions {
        item.widget_id(WidgetId::new(("tui canvas item", index)))
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
        item.place(
            Place::new()
                .fixed(unit.width * 2.0, unit.height)
                .layer(badges)
                .z_index(1),
        )
        .add(|ui: Cx<'_>| {
            let mut badge = ui
                .node(Flex::row().align(Align::Center).justify(Justify::Center))
                .insert(Block::new().background(colors::ACCENT_DARK))
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
    use blit_tui::color::Color;

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
