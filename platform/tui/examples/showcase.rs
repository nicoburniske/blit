use std::{cell::RefCell, fmt::Write as _, io, rc::Rc, time::Duration};

use blit::{
    Absolute, Anchor, Axis, Easing, Input, Interaction, Key, Place, Sense, Sides, Size, Sizing,
    Transition, Widget, WidgetId,
};
use blit_showcase::{CanvasConfig, CanvasLayout, FpsCounter, ITEMS, ItemSizing};
use blit_tui::{
    BoundsClip, ControlFlow, TuiPlatform, Ui,
    atom::{
        Bar, BarChart, Border, BorderSides, BorderStyle, Gauge, Shadow, Sparkline, TitlePosition,
    },
    color::Color,
    layout::{Align, Flex, Grid, Justify, Single, Wrap},
    text::{
        HorizontalAlign, Span, TextAttributes, TextOptions, TextOverflow, TextWrap, VerticalAlign,
    },
    widget::{Block, Text, Title, popover, resize, scroll, split},
};

fn main() -> io::Result<()> {
    let mut showcase = Showcase::default();
    blit_tui::run(|ui| showcase.show(ui))
}

struct Showcase {
    page: Page,
    layout: LayoutPage,
    text: TextPage,
    blocks: BlocksPage,
    atoms: AtomsPage,
    scroll: ScrollPage,
    settings: popover::State,
    fps: FpsBadge,
    show_fps: bool,
}

impl Default for Showcase {
    fn default() -> Self {
        Self {
            page: Page::default(),
            layout: LayoutPage::default(),
            text: TextPage::default(),
            blocks: BlocksPage::default(),
            atoms: AtomsPage::default(),
            scroll: ScrollPage::default(),
            settings: popover::State::new(),
            fps: FpsBadge::default(),
            show_fps: true,
        }
    }
}

impl Showcase {
    fn show(&mut self, ui: Ui<'_>) -> ControlFlow {
        let mut control = if matches!(ui.input(), Input::Text('q'))
            || matches!(ui.input(), Input::Key(key) if key.key == Key::Escape)
        {
            ControlFlow::Exit
        } else {
            ControlFlow::Continue
        };
        let mut root = ui.layout(Flex::column().padding(Sides::all(1.0)).gap(1.0));
        root.insert(Block::new().background(colors::BACKGROUND));
        {
            let mut header = root
                .child(Place::new().height(Sizing::fixed(1.0)))
                .layout(Flex::row().align(Align::Center).gap(1.0));
            header.insert(Block::new().background(colors::SURFACE));
            header.child(Place::new()).build(|ui: Ui<'_>| {
                let mut logo = ui.layout(Flex::row().align(Align::Center));
                logo.insert(Block::new().background(colors::ACCENT));
                logo.child(Place::new()).insert(
                    Text::new(" blit ")
                        .color(colors::SURFACE)
                        .attributes(TextAttributes::BOLD),
                );
            });
            for (page, label) in [
                (Page::Layout, " layout "),
                (Page::Text, " text "),
                (Page::Blocks, " blocks "),
                (Page::Atoms, " atoms "),
                (Page::Scroll, " scroll "),
            ] {
                if header.child(Place::new()).build(Button::new(
                    WidgetId::new(("tui page", label)),
                    label,
                    self.page == page,
                )) {
                    self.page = page;
                }
            }
            header.child(Place::grow()).insert(());
            let reset = header.child(Place::new()).build(
                popover::Popover::new(&mut self.settings)
                    .config(
                        popover::Config::new()
                            .target_anchor(Anchor::BottomRight)
                            .child_anchor(Anchor::TopRight)
                            .open_on_hover(true)
                            .close(popover::Close::Exit),
                    )
                    .trigger(|ui, interaction, open| {
                        draw_button(ui, " settings ", open, interaction);
                    })
                    .build(|ui: Ui<'_>| {
                        let mut popup = ui.layout(Flex::column().padding(Sides::all(1.0)).gap(1.0));
                        popup.insert(
                            Block::new()
                                .background(colors::SURFACE)
                                .border(Border::new(colors::ACCENT).style(BorderStyle::Rounded)),
                        );
                        if popup.child(Place::new()).build(Button::new(
                            WidgetId::new("tui settings show fps"),
                            " show fps ",
                            self.show_fps,
                        )) {
                            self.show_fps = !self.show_fps;
                        }
                        let reset = popup.child(Place::new()).build(Button::new(
                            WidgetId::new("tui reset showcase"),
                            " reset ",
                            false,
                        ));
                        if popup.child(Place::new()).build(Button::new(
                            WidgetId::new("tui quit showcase"),
                            " quit ",
                            false,
                        )) {
                            control = ControlFlow::Exit;
                        }
                        reset
                    }),
            );
            if reset.unwrap_or(false) {
                *self = Self::default();
            }
        }
        match self.page {
            Page::Layout => root.child(Place::grow()).build(&mut self.layout),
            Page::Text => root.child(Place::grow()).build(&mut self.text),
            Page::Blocks => root.child(Place::grow()).build(&mut self.blocks),
            Page::Atoms => root.child(Place::grow()).build(&mut self.atoms),
            Page::Scroll => root.child(Place::grow()).build(&mut self.scroll),
        };
        if self.show_fps {
            root.child(Place::absolute(
                Absolute::screen(0.0, 0.0)
                    .anchors(Anchor::BottomRight, Anchor::BottomRight)
                    .offset(-1.0, -1.0),
            ))
            .build(&mut self.fps);
        }
        control
    }
}

#[derive(Default)]
struct LayoutPage {
    canvas: CanvasConfig,
    resize: resize::State,
    scroll: scroll::State,
    split: split::State,
}

impl Widget<TuiPlatform> for &mut LayoutPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let LayoutPage {
            canvas,
            resize,
            scroll: layout_scroll,
            split,
        } = self;
        let screen = ui.screen();
        let preview_config = *canvas;
        let mut body = ui.layout(Flex::row());
        body.child(Place::grow()).build(SplitPane::new(
            split,
            WidgetId::new("tui layout page split"),
            40.0,
            |ui: Ui<'_>| {
                let mut sidebar = ui.layout(Flex::column().padding(Sides::all(1.0)));
                sidebar.insert(panel(colors::SURFACE, " LAYOUT PARAMETERS "));
                sidebar.child(Place::grow()).build(
                    ScrollArea::new(layout_scroll, BoundsClip).build(|ui: Ui<'_>| {
                        let mut controls = ui.layout(Flex::column().gap(1.0));
                        controls.child(Place::new()).insert(
                            Text::new("FLOW")
                                .color(colors::SECTION)
                                .attributes(TextAttributes::BOLD),
                        );
                        controls.child(Place::new()).build(|ui: Ui<'_>| {
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
                        controls.child(Place::new()).build(|ui: Ui<'_>| {
                            choices(
                                ui,
                                "axis",
                                &mut canvas.axis,
                                &[(" Horz ", Axis::Horizontal), (" Vert ", Axis::Vertical)],
                            );
                        });
                        controls.child(Place::new()).insert(
                            Text::new("DISTRIBUTION")
                                .color(colors::SECTION)
                                .attributes(TextAttributes::BOLD),
                        );
                        controls.child(Place::new()).build(|ui: Ui<'_>| {
                            choices(
                                ui,
                                "justify",
                                &mut canvas.justify,
                                &[
                                    (" Start ", Justify::Start),
                                    (" Center ", Justify::Center),
                                    (" End ", Justify::End),
                                    (" Between ", Justify::SpaceBetween),
                                    (" Around ", Justify::SpaceAround),
                                    (" Even ", Justify::SpaceEvenly),
                                ],
                            );
                        });
                        controls.child(Place::new()).build(|ui: Ui<'_>| {
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
                        controls.child(Place::new()).insert(
                            Text::new("SCALE, SPACE & MOTION")
                                .color(colors::SECTION)
                                .attributes(TextAttributes::BOLD),
                        );
                        controls.child(Place::new()).build(|ui: Ui<'_>| {
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
                        controls.child(Place::new()).build(|ui: Ui<'_>| {
                            choices(
                                ui,
                                "zoom",
                                &mut canvas.zoom,
                                &[(" 75% ", 0.75), (" 100% ", 1.0), (" 125% ", 1.25)],
                            );
                        });
                        controls.child(Place::new()).build(|ui: Ui<'_>| {
                            choices(
                                ui,
                                "gap",
                                &mut canvas.gap_steps,
                                &[(" 0 ", 0), (" 1 ", 1), (" 2 ", 2), (" 3 ", 3)],
                            );
                        });
                        controls.child(Place::new()).build(|ui: Ui<'_>| {
                            choices(
                                ui,
                                "padding",
                                &mut canvas.padding_steps,
                                &[(" 0 ", 0), (" 1 ", 1), (" 2 ", 2), (" 3 ", 3)],
                            );
                        });
                        controls.child(Place::new()).build(|ui: Ui<'_>| {
                            choices(
                                ui,
                                "transitions",
                                &mut canvas.transitions,
                                &[(" On ", true), (" Off ", false)],
                            );
                        });
                    }),
                );
            },
            |ui: Ui<'_>| {
                let mut preview = ui.layout(Flex::column().padding(Sides::all(1.0)));
                preview.insert(
                    panel(colors::SURFACE, " LIVE PREVIEW ").title(
                        Title::new(match (preview_config.layout, preview_config.axis) {
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
                        .child(Place::grow())
                        .layout(Single::new().padding(Sides::all(1.0)))
                        .clip(BoundsClip);
                    viewport.insert(Block::new().background(colors::TRACK));
                    viewport.child(Place::new()).build(
                        resize::Area::new(
                            resize,
                            WidgetId::new("tui layout canvas"),
                            Size::new(
                                ((screen.width - 48.0) * 0.8).max(18.0),
                                ((screen.height - 8.0) * 0.72).max(9.0),
                            ),
                            Canvas {
                                config: preview_config,
                            },
                            TuiGrip,
                        )
                        .minimum(Size::new(18.0, 9.0))
                        .maximum(screen.size())
                        .grip_size(Size::uniform(1.0)),
                    );
                }
            },
        ));
    }
}

struct TextPage {
    resize: resize::State,
    attributes: TextAttributes,
    wrap: TextWrap,
    overflow: TextOverflow,
    horizontal: HorizontalAlign,
    vertical: VerticalAlign,
    max_lines: Option<u16>,
    split: split::State,
}

impl Default for TextPage {
    fn default() -> Self {
        Self {
            resize: resize::State::default(),
            attributes: TextAttributes::NONE,
            wrap: TextWrap::Word,
            overflow: TextOverflow::Clip,
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
            max_lines: None,
            split: split::State::default(),
        }
    }
}

impl Widget<TuiPlatform> for &mut TextPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let TextPage {
            resize: text_resize,
            attributes: text_attributes,
            wrap: text_wrap,
            overflow: text_overflow,
            horizontal: text_horizontal,
            vertical: text_vertical,
            max_lines: text_max_lines,
            split,
        } = self;
        let screen = ui.screen();
        let preview_attributes = *text_attributes;
        let preview_wrap = *text_wrap;
        let preview_overflow = *text_overflow;
        let preview_horizontal = *text_horizontal;
        let preview_vertical = *text_vertical;
        let preview_max_lines = *text_max_lines;
        let mut body = ui.layout(Flex::row());
        body.child(Place::grow()).build(SplitPane::new(
            split,
            WidgetId::new("tui text page split"),
            40.0,
            |ui: Ui<'_>| {
                let mut controls = ui.layout(Flex::column().padding(Sides::all(1.0)).gap(1.0));
                controls.insert(panel(colors::SURFACE, " TEXT CONTROLS "));
                controls.child(Place::new()).insert(
                    Text::new("ATTRIBUTES")
                        .color(colors::SECTION)
                        .attributes(TextAttributes::BOLD),
                );
                controls.child(Place::new()).build(|ui: Ui<'_>| {
                    let mut toggles = ui.layout(
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
                        if toggles.child(Place::new()).build(Button::new(
                            WidgetId::new(("tui text attribute", index)),
                            label,
                            selected,
                        )) {
                            text_attributes.set(attribute, !selected);
                        }
                    }
                });
                controls.child(Place::new()).insert(
                    Text::new("OPTIONS")
                        .color(colors::SECTION)
                        .attributes(TextAttributes::BOLD),
                );
                controls.child(Place::new()).build(|ui: Ui<'_>| {
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
                controls.child(Place::new()).build(|ui: Ui<'_>| {
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
                controls.child(Place::new()).build(|ui: Ui<'_>| {
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
                controls.child(Place::new()).build(|ui: Ui<'_>| {
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
                controls.child(Place::new()).build(|ui: Ui<'_>| {
                    choices(
                        ui,
                        "maximum lines",
                        text_max_lines,
                        &[(" All ", None), (" 3 ", Some(3)), (" 6 ", Some(6))],
                    );
                });
            },
            |ui: Ui<'_>| {
                let mut preview = ui.layout(Flex::column().padding(Sides::all(1.0)).gap(1.0));
                preview.insert(
                    panel(colors::SURFACE, " RESIZABLE TEXT ").title(
                        Title::new(" DRAG TO REFLOW ")
                            .color(colors::TEXT_DIM)
                            .position(TitlePosition::BottomRight),
                    ),
                );
                {
                    let mut viewport = preview
                        .child(Place::grow())
                        .layout(Single::new().padding(Sides::all(1.0)))
                        .clip(BoundsClip);
                    viewport.insert(Block::new().background(colors::TRACK));
                    let mut options = TextOptions::new()
                        .wrap(preview_wrap)
                        .overflow(preview_overflow)
                        .horizontal_align(preview_horizontal)
                        .vertical_align(preview_vertical);
                    if let Some(max_lines) = preview_max_lines {
                        options = options.max_lines(max_lines);
                    }
                    viewport.child(Place::new()).build(
                        resize::Area::new(
                            text_resize,
                            WidgetId::new("tui text preview"),
                            Size::new(64.0, 16.0),
                            |ui: Ui<'_>| {
                                let mut paragraph = ui
                                    .layout(Flex::column().padding(Sides::all(1.0)))
                                    .clip(BoundsClip);
                                paragraph.insert(
                                    Block::new().background(colors::CANVAS).border(
                                        Border::new(colors::CANVAS_BORDER).style(BorderStyle::Rounded),
                                    ),
                                );
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
                                paragraph.child(Place::grow()).insert(
                                    Text::rich(&sample)
                                        .color(colors::TEXT)
                                        .attributes(preview_attributes)
                                        .options(options),
                                );
                            },
                            TuiGrip,
                        )
                        .minimum(Size::new(12.0, 6.0))
                        .maximum(screen.size())
                        .grip_size(Size::uniform(1.0)),
                    );
                }
            },
        ));
    }
}

struct BlocksPage {
    style: BorderStyle,
    sides: BorderSides,
    shadow: bool,
    background: bool,
    split: split::State,
}

impl Default for BlocksPage {
    fn default() -> Self {
        Self {
            style: BorderStyle::Rounded,
            sides: BorderSides::ALL,
            shadow: true,
            background: true,
            split: split::State::default(),
        }
    }
}

impl Widget<TuiPlatform> for &mut BlocksPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let BlocksPage {
            style: block_style,
            sides: block_sides,
            shadow: block_shadow,
            background: block_background,
            split,
        } = self;
        let preview_style = *block_style;
        let preview_sides = *block_sides;
        let preview_shadow = *block_shadow;
        let preview_background = *block_background;
        let mut body = ui.layout(Flex::row());
        body.child(Place::grow()).build(SplitPane::new(
            split,
            WidgetId::new("tui blocks page split"),
            40.0,
            |ui: Ui<'_>| {
                let mut controls = ui.layout(Flex::column().padding(Sides::all(1.0)).gap(1.0));
                controls.insert(panel(colors::SURFACE, " BLOCK OPTIONS "));
                controls.child(Place::new()).build(|ui: Ui<'_>| {
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
                controls.child(Place::new()).build(|ui: Ui<'_>| {
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
                controls.child(Place::new()).build(|ui: Ui<'_>| {
                    choices(
                        ui,
                        "shadow",
                        block_shadow,
                        &[(" On ", true), (" Off ", false)],
                    );
                });
                controls.child(Place::new()).build(|ui: Ui<'_>| {
                    choices(
                        ui,
                        "background",
                        block_background,
                        &[(" On ", true), (" Off ", false)],
                    );
                });
            },
            |ui: Ui<'_>| {
                let mut preview = ui.layout(Flex::column().padding(Sides::all(1.0)).gap(1.0));
                preview.insert(panel(colors::SURFACE, " BLOCK PREVIEW "));
                preview.child(Place::grow()).build(|ui: Ui<'_>| {
                    let mut block = Block::new()
                        .border(
                            Border::new(colors::CANVAS_BORDER)
                                .style(preview_style)
                                .sides(preview_sides),
                        )
                        .title(
                            Title::new(" CONFIGURED BLOCK ")
                                .color(colors::ACCENT)
                                .attributes(TextAttributes::BOLD),
                        );
                    if preview_background {
                        block = block.background(colors::SURFACE_HIGH);
                    }
                    if preview_shadow {
                        block = block.shadow(Shadow::new(colors::SHADOW));
                    }
                    let mut configured = ui.layout(
                        Flex::column()
                            .padding(Sides::all(1.0))
                            .align(Align::Center)
                            .justify(Justify::Center),
                    );
                    configured.insert(block);
                    configured.child(Place::new()).insert(
                        Text::new("change the options on the left").color(colors::TEXT_MUTED),
                    );
                });
            },
        ));
    }
}

struct AtomsPage {
    sparkline: Rc<RefCell<Vec<u64>>>,
    bars: Rc<RefCell<Vec<Bar>>>,
}

impl Default for AtomsPage {
    fn default() -> Self {
        Self {
            sparkline: Rc::new(RefCell::new(vec![0; 80])),
            bars: Rc::new(RefCell::new(vec![
                Bar::new(32, "Mon".into()),
                Bar::new(67, "Tue".into()),
                Bar::new(45, "Wed".into()),
                Bar::new(86, "Thu".into()),
                Bar::new(58, "Fri".into()),
                Bar::new(93, "Sat".into()),
                Bar::new(74, "Sun".into()),
            ])),
        }
    }
}

impl Widget<TuiPlatform> for &mut AtomsPage {
    type Response = ();

    fn build(self, mut ui: Ui<'_>) {
        let phase = ui.animate_loop(
            WidgetId::new("tui atom animation"),
            Duration::from_secs(3),
            Easing::Linear,
        );
        let ratio = (phase * std::f32::consts::TAU).sin() * 0.25 + 0.5;
        for (x, value) in self.sparkline.borrow_mut().iter_mut().enumerate() {
            *value =
                (((x as f32 * 0.24 + phase * std::f32::consts::TAU).sin() + 1.0) * 50.0) as u64;
        }

        let mut body = ui.layout(Flex::column().padding(Sides::all(1.0)).gap(1.0));
        body.insert(panel(colors::SURFACE, " DIRECT CELL ATOMS "));
        body.child(Place::new()).insert(
            Text::new("ratatui-style atoms paint into the resolved atom area")
                .color(colors::TEXT_MUTED),
        );
        body.child(Place::new().height(Sizing::fixed(1.0))).insert(
            Gauge::new(ratio as f64)
                .filled(colors::ACCENT)
                .unfilled(colors::TRACK)
                .label_color(colors::SURFACE),
        );
        body.child(Place::new().height(Sizing::fixed(5.0))).insert(
            Sparkline::new(self.sparkline.clone())
                .maximum(100)
                .color(colors::ACCENT)
                .background(colors::CANVAS),
        );
        body.child(Place::grow()).insert(
            BarChart::new(self.bars.clone())
                .maximum(100)
                .bar_width(5)
                .gap(2)
                .color(colors::SECTION)
                .label_color(colors::TEXT_MUTED)
                .background(colors::CANVAS),
        );
    }
}

struct ScrollPage {
    axis: Axis,
    state: scroll::State,
    items: Vec<String>,
}

impl Default for ScrollPage {
    fn default() -> Self {
        Self {
            axis: Axis::default(),
            state: scroll::State::default(),
            items: (1..=100).map(|index| index.to_string()).collect(),
        }
    }
}

impl Widget<TuiPlatform> for &mut ScrollPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let ScrollPage {
            axis: scroll_axis,
            state: scroll,
            items,
        } = self;
        let mut body = ui.layout(Flex::column().padding(Sides::all(1.0)).gap(1.0));
        body.insert(panel(colors::SURFACE, " SCROLL AREA "));
        {
            let mut controls = body
                .child(Place::new())
                .layout(Flex::row().gap(1.0).align(Align::Center));
            controls.child(Place::new()).insert(
                Text::new("AXIS")
                    .color(colors::TEXT_MUTED)
                    .attributes(TextAttributes::BOLD),
            );
            for (axis, label) in [
                (Axis::Vertical, " Vertical "),
                (Axis::Horizontal, " Horizontal "),
            ] {
                if controls.child(Place::new()).build(Button::new(
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
        let item_extent = match axis {
            Axis::Horizontal => 12.0,
            Axis::Vertical => 3.0,
        };
        body.child(Place::grow()).build(
            ScrollList::new(scroll, BoundsClip, items.iter().enumerate(), item_extent)
                .build(move |ui: Ui<'_>, (index, item)| {
                    let layout = match axis {
                        Axis::Horizontal => {
                            Flex::column().align(Align::Center).justify(Justify::Center)
                        }
                        Axis::Vertical => Flex::row().padding(Sides::all(1.0)).align(Align::Center),
                    };
                    let background = if index.is_multiple_of(2) {
                        colors::SURFACE_HIGH
                    } else {
                        colors::TRACK
                    };
                    let mut tile = ui.layout(layout);
                    tile.insert(
                        Block::new()
                            .background(background)
                            .border(Border::new(colors::CANVAS_BORDER)),
                    );
                    tile.child(Place::new()).insert(
                        Text::new(item)
                            .color(colors::TEXT)
                            .attributes(TextAttributes::BOLD),
                    );
                })
                .axis(axis)
                .gap(0.0),
        );
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Layout,
    Text,
    Blocks,
    Atoms,
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

    fn build(self, ui: Ui<'_>) {
        if let Some(fps) = self.counter.update(ui.time()) {
            self.label.clear();
            let _ = write!(self.label, "FPS {fps:03.0}");
        }
        let mut badge = ui.layout(
            Flex::row()
                .padding(Sides::all(1.0))
                .gap(1.0)
                .align(Align::Center),
        );
        badge.insert(
            Block::new()
                .background(colors::SURFACE_HIGH)
                .border(Border::new(colors::ACCENT).style(BorderStyle::Rounded)),
        );
        badge
            .child(Place::fixed(1.0, 1.0))
            .insert(Block::new().background(colors::ACCENT));
        badge.child(Place::new()).insert(
            Text::new(&self.label)
                .color(colors::TEXT)
                .attributes(TextAttributes::BOLD),
        );
        badge
            .child(Place::new())
            .insert(Text::new("SCREEN ABSOLUTE").color(colors::TEXT_DIM));
    }
}

struct TuiGrip(resize::Grip);

impl Widget<TuiPlatform> for TuiGrip {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let marker = match self.0.edge {
            resize::Edge::Right => "║",
            resize::Edge::Bottom => "═══",
            resize::Edge::Corner => "╝",
        };
        let active =
            self.0.interaction.hovered || self.0.interaction.active || self.0.interaction.dragging;
        let color = if active {
            colors::ACCENT
        } else if self.0.edge == resize::Edge::Corner {
            colors::GRIP_CORNER
        } else {
            colors::GRIP
        };
        let mut grip = ui.layout(Flex::row().align(Align::Center).justify(Justify::Center));
        grip.child(Place::new())
            .insert(Text::new(marker).color(color));
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

    fn build(self, mut ui: Ui<'_>) -> bool {
        let interaction = ui.interact(self.id, Sense::CLICK);
        ui.widget_id(self.id).build(|ui: Ui<'_>| {
            draw_button(ui, self.label, self.selected, interaction);
        });
        interaction.clicked
    }
}

fn draw_button(ui: Ui<'_>, label: &str, selected: bool, interaction: Interaction) {
    let block = if interaction.active {
        Block::new().background(colors::ACCENT_DARK)
    } else if selected {
        Block::new().background(colors::SELECTED)
    } else if interaction.hovered {
        Block::new().background(colors::SURFACE_HIGH)
    } else {
        Block::new()
    };
    let mut button = ui.layout(Flex::row().padding(Sides::x(1.0)));
    button.insert(block);
    button
        .child(Place::new())
        .insert(Text::new(label).color(colors::TEXT));
}

fn choices<T: Copy + PartialEq>(ui: Ui<'_>, label: &str, selected: &mut T, options: &[(&str, T)]) {
    let mut group = ui.layout(Flex::column());
    group
        .child(Place::new())
        .insert(Text::new(label).color(colors::TEXT_MUTED));
    group.child(Place::new()).build(|ui: Ui<'_>| {
        let mut values = ui.layout(
            Wrap::new(Axis::Horizontal)
                .item_gap(2.0)
                .run_gap(1.0)
                .align(Align::Center),
        );
        for (index, &(option, value)) in options.iter().enumerate() {
            let clicked = values.child(Place::new()).build(Button::new(
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

    fn build(self, ui: Ui<'_>) {
        let unit = Size::uniform(1.0);
        let background = Block::new()
            .background(colors::CANVAS)
            .border(Border::new(colors::CANVAS_BORDER).style(BorderStyle::Rounded));
        match self.config.layout {
            CanvasLayout::Flex => {
                let mut canvas = ui
                    .layout(
                        Flex::new(self.config.axis)
                            .padding(self.config.padding(unit))
                            .gap(self.config.gap(self.config.axis, unit))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .clip(BoundsClip);
                canvas.insert(background);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    canvas
                        .child(self.config.item_place(index, unit))
                        .build(|ui: Ui<'_>| {
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
                    .layout(
                        Wrap::new(self.config.axis)
                            .padding(self.config.padding(unit))
                            .item_gap(self.config.gap(self.config.axis, unit))
                            .run_gap(self.config.gap(cross, unit))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .clip(BoundsClip);
                canvas.insert(background);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    canvas
                        .child(self.config.item_place(index, unit))
                        .build(|ui: Ui<'_>| {
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
                let mut canvas = ui.layout(grid).clip(BoundsClip);
                canvas.insert(background);
                let badges = canvas.new_layer();
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    let item = placer.place(spec.rows, spec.columns);
                    canvas
                        .child(Place::item(item).height(Sizing::fixed(3.0 * self.config.zoom)))
                        .build(|ui: Ui<'_>| {
                            canvas_item(ui, index, spec, badges, self.config, unit);
                        });
                }
            }
        }
    }
}

fn canvas_item(
    ui: Ui<'_>,
    index: usize,
    spec: blit_showcase::ItemSpec,
    badges: blit::LayerId,
    config: CanvasConfig,
    unit: Size,
) {
    let mut item = ui.layout(Flex::column().align(Align::Center).justify(Justify::Center));
    item.insert(Block::new().background(colors::ITEMS[index]));
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
    item.child(Place::new()).insert(
        Text::new(spec.label)
            .color(colors::TEXT)
            .attributes(TextAttributes::BOLD),
    );
    if let Some(anchor) = spec.badge {
        item.child(
            Place::absolute(Absolute::attach(anchor, Anchor::Center))
                .width(Sizing::fixed(unit.width * 2.0))
                .height(Sizing::fixed(unit.height))
                .layer(badges)
                .z_index(1),
        )
        .build(|ui: Ui<'_>| {
            let mut badge = ui.layout(Flex::row().align(Align::Center).justify(Justify::Center));
            badge.insert(Block::new().background(colors::ACCENT_DARK));
            badge
                .child(Place::new())
                .insert(Text::new("A").color(colors::TEXT));
        });
    }
}

#[derive(Clone, Copy, Default)]
struct Split;

type SplitPane<'a, L, T> = split::Pane<'a, L, T, Split>;

impl split::Divider for Split {
    type Widget = Divider;

    fn into_widget(self, _: Axis, interaction: Interaction) -> Self::Widget {
        Divider(interaction)
    }
}

struct Divider(Interaction);

impl Widget<TuiPlatform> for Divider {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let active = self.0.hovered || self.0.dragging;
        let mut divider = ui.layout(Flex::row().align(Align::Center).justify(Justify::Center));
        divider
            .child(Place::new())
            .insert(Text::new("⠿").color(if active {
                colors::ACCENT
            } else {
                colors::TEXT_DIM
            }));
    }
}

#[derive(Clone, Copy, Default)]
struct Scroll;

type ScrollArea<'a, C = ()> = scroll::Area<'a, TuiPlatform, BoundsClip, Scroll, C>;
type ScrollList<'a, I, F = ()> = scroll::List<'a, TuiPlatform, I, BoundsClip, Scroll, F>;

impl scroll::Scrollbar for Scroll {
    const HAS_TRACK: bool = true;
    const HAS_THUMB: bool = true;

    type Track = Block<'static>;
    type Thumb = Block<'static>;

    fn config(&self) -> scroll::Config {
        scroll::Config::new().minimum_thumb_extent(4.0)
    }

    fn into_content(self, active: bool) -> (Self::Track, Self::Thumb) {
        (
            Block::new().background(colors::TRACK),
            Block::new().background(if active {
                colors::ACCENT
            } else {
                colors::ACCENT_DARK
            }),
        )
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
