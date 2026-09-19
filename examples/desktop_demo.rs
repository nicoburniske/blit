use std::time::Duration;

use blit::{
    Absolute, Anchor, Axis, Easing, Interaction, Point, Sense, Sides, Size, Sizing, Transition,
    Widget, WidgetId,
};
use blit_demo::{CanvasConfig, CanvasLayout, ITEMS, ItemSizing};
#[cfg(not(feature = "gpu"))]
use blit_desktop::cpu;
#[cfg(feature = "gpu")]
use blit_desktop::gpu;
use blit_desktop::{Application, Config, EventLoopProxy, Root};
use blit_gui::{
    BoundsClip, FontData, FontFamily, GuiContext, TextConfig, TextLayoutEngine, Ui,
    atom::{Rectangle, Shadow},
    color::Color,
    layout::{Align, flex, grid, single, wrap},
    style::{Border, BorderRadius},
    text::{
        FontId, FontStyle, HorizontalAlign, Span, TextOptions, TextOverflow, TextStyle, TextWrap,
        VerticalAlign,
    },
    widget::{
        Performance, RichText, Text, TextInput, performance, popover, resize, scroll_area,
        scroll_list, split, text_input,
    },
};

pub fn run(text: impl TextLayoutEngine) {
    let fonts = std::fs::read_dir(std::path::Path::new(env!("BLIT_TEST_FONT")).with_file_name(""))
        .unwrap()
        .filter_map(|entry| std::fs::read(entry.ok()?.path()).ok())
        .map(|data| FontData::Shared(data.into()))
        .collect();
    #[cfg(feature = "gpu")]
    let graphics = Box::new(gpu::Backend::new(gpu::Config::default()));
    #[cfg(not(feature = "gpu"))]
    let graphics = Box::new(cpu::Backend::new(cpu::Config {
        paint_cache_capacity: 2 * 1024 * 1024,
        glyph_cache_capacity: 1024 * 1024,
        shadow_cache_capacity: 512 * 1024,
    }));
    blit_desktop::run::<App>(Config {
        title: "Blit layout playground".into(),
        width: 1120,
        height: 800,
        text_config: TextConfig {
            fonts: vec![FontFamily {
                id: FontId::default(),
                fonts,
            }],
            text_cache_capacity: 1024 * 1024,
            layout_cache_capacity: 2 * 1024 * 1024,
        },
        text,
        graphics,
    })
    .unwrap();
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Layout,
    Text,
    Input,
    Styles,
    Scroll,
}

struct App {
    page: Page,
    layout: LayoutPage,
    text: TextPage,
    input: InputPage,
    styles: StylesPage,
    scroll: ScrollPage,
    settings: popover::State,
    performance: performance::State,
    show_performance: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            page: Page::default(),
            layout: LayoutPage::default(),
            text: TextPage::default(),
            input: InputPage::default(),
            styles: StylesPage::default(),
            scroll: ScrollPage::default(),
            settings: popover::State::new(),
            performance: performance::State::default(),
            show_performance: true,
        }
    }
}

impl Application for App {
    type Input = ();

    fn new(_: EventLoopProxy<Self::Input>, _: Root<Self>, _: &mut GuiContext) -> Self {
        Self::default()
    }

    fn input(&mut self, _: Self::Input) {}

    fn render(&mut self, ui: Ui<'_>) {
        let mut root = ui.layout(flex::column().padding(Sides::all(sz::XL)).gap(sz::LG));
        root.insert(Rectangle::new().background(colors::BACKGROUND));
        {
            let mut header = root
                .child(flex::item().height(Sizing::fixed(sz::XXXL)))
                .layout(
                    flex::row()
                        .padding(Sides::xy(sz::SM, sz::XS))
                        .gap(sz::XS)
                        .align(Align::Center),
                );
            header.insert(
                Rectangle::new()
                    .background(colors::SURFACE)
                    .border(Border::solid(sz::BORDER, colors::BORDER))
                    .radius(BorderRadius::uniform(sz::XS)),
            );
            header.child(flex::item()).build(|ui: Ui<'_>| {
                let mut logo = ui.layout(
                    flex::row()
                        .padding(Sides::xy(sz::SM, sz::XXS))
                        .align(Align::Center),
                );
                logo.insert(
                    Rectangle::new()
                        .background(colors::ACCENT)
                        .radius(BorderRadius::uniform(sz::XXS)),
                );
                logo.child(flex::item()).insert(
                    Text::new("blit")
                        .style(TextStyle {
                            size: sz::LG,
                            ..TextStyle::default()
                        })
                        .color(colors::BACKGROUND),
                );
            });
            for (page, label) in [
                (Page::Layout, "layout"),
                (Page::Text, "text"),
                (Page::Input, "input"),
                (Page::Styles, "styles"),
                (Page::Scroll, "scroll"),
            ] {
                if header.child(flex::item()).build(Button::new(
                    WidgetId::new(("desktop page", label)),
                    label,
                    self.page == page,
                )) {
                    self.page = page;
                }
            }
            header.child(flex::item().grow()).insert(());
            let reset = header.child(flex::item()).build(popover::new(
                &mut self.settings,
                popover::Config::new()
                    .target_anchor(Anchor::BottomRight)
                    .child_anchor(Anchor::TopRight)
                    .offset(Point::new(0.0, sz::XXS))
                    .open_on_hover(true)
                    .close(popover::Close::Exit),
                |ui, interaction, open| {
                    draw_button(ui, "settings", open, interaction);
                },
                |ui: Ui<'_>| {
                    let mut popup =
                        ui.layout(flex::column().padding(Sides::all(sz::MD)).gap(sz::SM));
                    popup.insert(
                        Rectangle::new()
                            .background(colors::SURFACE)
                            .border(Border::solid(sz::BORDER, colors::ACCENT))
                            .radius(BorderRadius::uniform(sz::XS)),
                    );
                    if popup.child(flex::item()).build(Button::new(
                        WidgetId::new("desktop settings show performance"),
                        "Show performance",
                        self.show_performance,
                    )) {
                        self.show_performance = !self.show_performance;
                    }
                    popup.child(flex::item()).build(Button::new(
                        WidgetId::new("reset desktop demo"),
                        "Reset",
                        false,
                    ))
                },
            ));
            if reset.unwrap_or(false) {
                *self = Self::default();
            }
        }
        match self.page {
            Page::Layout => root.child(flex::item().grow()).build(&mut self.layout),
            Page::Text => root.child(flex::item().grow()).build(&mut self.text),
            Page::Input => root.child(flex::item().grow()).build(&mut self.input),
            Page::Styles => root.child(flex::item().grow()).build(&mut self.styles),
            Page::Scroll => root.child(flex::item().grow()).build(&mut self.scroll),
        };
        if self.show_performance {
            root.absolute(
                Absolute::screen(0.0, 0.0)
                    .anchors(Anchor::BottomRight, Anchor::BottomRight)
                    .offset(-sz::LG, -sz::LG),
            )
            .build(
                Performance::new(&mut self.performance)
                    .background(colors::SURFACE_HIGH)
                    .graph_background(colors::CANVAS)
                    .color(colors::TEXT)
                    .muted_color(colors::TEXT_DIM)
                    .accent(colors::ACCENT),
            );
        }
    }
}

struct TextPage {
    resize: resize::State,
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
            wrap: TextWrap::Word,
            overflow: TextOverflow::Clip,
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
            max_lines: None,
            split: split::State::default(),
        }
    }
}

impl Widget<GuiContext> for &mut TextPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let TextPage {
            resize,
            wrap,
            overflow,
            horizontal,
            vertical,
            max_lines,
            split: split_state,
        } = self;
        let options = TextOptions {
            wrap: *wrap,
            overflow: *overflow,
            horizontal_align: *horizontal,
            vertical_align: *vertical,
            max_lines: *max_lines,
        };
        let screen = ui.screen().size();
        let mut body = ui.layout(flex::row());
        body.child(flex::item().grow()).build(split(
            split_state,
            WidgetId::new("text page split"),
            split::Config::new(sz::SIDEBAR).divider_extent(sz::LG),
            |ui: Ui<'_>| {
                let mut controls =
                    ui.layout(flex::column().padding(Sides::all(sz::LG)).gap(sz::SM));
                controls.insert(panel(colors::SURFACE));
                controls.child(flex::item()).insert(
                    Text::new("TEXT OPTIONS")
                        .style(TextStyle {
                            size: sz::LG,
                            ..TextStyle::default()
                        })
                        .color(colors::ACCENT),
                );
                controls.child(flex::item()).build(|ui: Ui<'_>| {
                    choices(
                        ui,
                        "wrap",
                        wrap,
                        &[
                            ("None", TextWrap::None),
                            ("Word", TextWrap::Word),
                            ("Character", TextWrap::Character),
                        ],
                    );
                });
                controls.child(flex::item()).build(|ui: Ui<'_>| {
                    choices(
                        ui,
                        "overflow",
                        overflow,
                        &[("Clip", TextOverflow::Clip), ("Ellipsis", TextOverflow::Ellipsis)],
                    );
                });
                controls.child(flex::item()).build(|ui: Ui<'_>| {
                    choices(
                        ui,
                        "horizontal",
                        horizontal,
                        &[
                            ("Left", HorizontalAlign::Left),
                            ("Center", HorizontalAlign::Center),
                            ("Right", HorizontalAlign::Right),
                        ],
                    );
                });
                controls.child(flex::item()).build(|ui: Ui<'_>| {
                    choices(
                        ui,
                        "vertical",
                        vertical,
                        &[
                            ("Top", VerticalAlign::Top),
                            ("Center", VerticalAlign::Center),
                            ("Bottom", VerticalAlign::Bottom),
                        ],
                    );
                });
                controls.child(flex::item()).build(|ui: Ui<'_>| {
                    choices(
                        ui,
                        "maximum lines",
                        max_lines,
                        &[("All", None), ("3", Some(3)), ("6", Some(6))],
                    );
                });
                controls.child(flex::item()).insert(
                    Text::new("Drag the right edge, bottom edge, or corner of the paragraph to reflow it.")
                        .style(TextStyle {
                            size: sz::MD,
                            ..TextStyle::default()
                        })
                        .color(colors::TEXT_DIM)
                        .options(TextOptions {
                            wrap: TextWrap::Word,
                            ..TextOptions::default()
                        }),
                );
            },
            |ui: Ui<'_>| {
                let mut preview =
                    ui.layout(flex::column().padding(Sides::all(sz::LG)).gap(sz::SM));
                preview.insert(panel(colors::SURFACE));
                preview.child(flex::item()).insert(
                    Text::new("RESIZABLE RICH TEXT")
                        .style(TextStyle {
                            size: sz::MD,
                            ..TextStyle::default()
                        })
                        .color(colors::ACCENT),
                );
                preview.child(flex::item().grow()).build(|ui: Ui<'_>| {
                    let mut viewport = ui
                        .layout(single::layout().padding(Sides::all(sz::SM)))
                        .clip(BoundsClip);
                    viewport.insert(
                        Rectangle::new()
                            .background(colors::TRACK)
                            .radius(BorderRadius::uniform(sz::XS)),
                    );
                    viewport.child(single::item()).build(
                        resize::new(
                            resize,
                            WidgetId::new("rich text preview"),
                            resize::Config::new(Size::new(560.0, 360.0))
                                .minimum(Size::new(260.0, 160.0))
                                .maximum(screen)
                                .grip_size(Size::uniform(sz::MD)),
                            |ui: Ui<'_>| {
                                let mut paragraph = ui
                                    .layout(single::layout().padding(Sides::all(sz::LG)))
                                    .clip(BoundsClip);
                                paragraph.insert(
                                    Rectangle::new()
                                        .background(colors::CANVAS)
                                        .border(Border::solid(
                                            sz::BORDER,
                                            colors::CANVAS_BORDER,
                                        ))
                                        .radius(BorderRadius::uniform(sz::XS)),
                                );
                                let sample = [
                                    Span::new("Rich text\n")
                                        .size(sz::XXL)
                                        .weight(700)
                                        .style(FontStyle::Italic)
                                        .color(colors::ACCENT),
                                    Span::new("One paragraph can mix inherited body text with "),
                                    Span::new("large type")
                                        .size(sz::XL)
                                        .color(colors::TEXT),
                                    Span::new(", "),
                                    Span::new("bold").weight(700).color(colors::TEXT),
                                    Span::new(", "),
                                    Span::new("italic")
                                        .style(FontStyle::Italic)
                                        .color(colors::TEXT),
                                    Span::new(", "),
                                    Span::new("oblique")
                                        .style(FontStyle::Oblique)
                                        .color(colors::TEXT),
                                    Span::new(", and "),
                                    Span::new("small details")
                                        .size(sz::SM)
                                        .color(colors::TEXT_MUTED),
                                    Span::new(
                                        ". Every styled span participates in the same wrapping, alignment, measurement, clipping, and ellipsis behavior. Resize the panel to watch the whole paragraph reflow.",
                                    ),
                                ];
                                paragraph.child(single::item().grow()).insert(
                                    RichText::new(&sample)
                                        .style(TextStyle {
                                            size: sz::LG,
                                            ..TextStyle::default()
                                        })
                                        .color(colors::TEXT_MUTED)
                                        .options(options),
                                );
                            },
                            DesktopGrip,
                        ),
                    );
                });
            },
        ));
    }
}

#[derive(Default)]
struct InputPage {
    value: String,
    state: text_input::State,
}

impl Widget<GuiContext> for &mut InputPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let mut body = ui.layout(flex::column().padding(Sides::all(sz::LG)).gap(sz::SM));
        body.insert(panel(colors::SURFACE));
        body.child(flex::item()).insert(
            Text::new("TEXT INPUT")
                .style(TextStyle {
                    size: sz::LG,
                    ..TextStyle::default()
                })
                .color(colors::ACCENT),
        );
        body.child(
            flex::item()
                .width(Sizing::fixed(560.0))
                .height(Sizing::fixed(sz::XXXL)),
        )
        .build(|ui: Ui<'_>| {
            let mut field = ui.layout(single::layout());
            field.insert(
                Rectangle::new()
                    .background(colors::TRACK)
                    .border(Border::solid(sz::BORDER, colors::BORDER))
                    .radius(BorderRadius::uniform(sz::XS)),
            );
            field.child(single::item().grow()).build(
                TextInput::new(
                    &mut self.state,
                    WidgetId::new("desktop text input"),
                    &mut self.value,
                )
                .style(TextStyle {
                    size: sz::LG,
                    ..TextStyle::default()
                })
                .padding(Sides::all(sz::SM))
                .color(colors::TEXT)
                .placeholder("Type here")
                .placeholder_color(colors::TEXT_DIM)
                .selection_background(colors::ACCENT_DARK)
                .cursor_background(colors::ACCENT),
            )
        });
        body.child(flex::item()).insert(
            Text::new("Click to focus. Escape releases focus.")
                .style(TextStyle {
                    size: sz::MD,
                    ..TextStyle::default()
                })
                .color(colors::TEXT_MUTED),
        );
    }
}

#[derive(Default)]
struct LayoutPage {
    canvas: CanvasConfig,
    resize: resize::State,
    scroll: scroll_area::State,
    split: split::State,
}

impl Widget<GuiContext> for &mut LayoutPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let LayoutPage {
            canvas,
            resize,
            scroll: controls_scroll,
            split: split_state,
        } = self;
        let screen = ui.screen().size();
        let unit = Size::uniform(sz::SM);
        let preview_config = *canvas;
        let mut body = ui.layout(flex::row());
        body.child(flex::item().grow()).build(
            split(
                split_state,
                WidgetId::new("layout page split"),
                split::Config::new(sz::SIDEBAR).divider_extent(sz::LG),
                |ui: Ui<'_>| {
                    let mut sidebar = ui.layout(
                        flex::column()
                            .padding(Sides::all(sz::LG))
                            .gap(sz::XS),
                    );
                    sidebar.insert(panel(colors::SURFACE));
                    sidebar.child(flex::item()).insert(
                        Text::new("LAYOUT PARAMETERS")
                            .style(TextStyle {
                                size: sz::LG,
                                ..TextStyle::default()
                            })
                            .color(colors::ACCENT),
                    );
                    sidebar.child(flex::item().grow()).build(
                        scroll_area(
                            controls_scroll,
                            scroll_area::Config::new(),
                            |ui: Ui<'_>| {
                            let mut controls = ui.layout(flex::column().gap(sz::XS));
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "layout",
                                    &mut canvas.layout,
                                    &[
                                        ("Flex", CanvasLayout::Flex),
                                        ("Wrap", CanvasLayout::Wrap),
                                        ("Grid", CanvasLayout::Grid),
                                    ],
                                );
                            });
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "axis",
                                    &mut canvas.axis,
                                    &[("Horizontal", Axis::Horizontal), ("Vertical", Axis::Vertical)],
                                );
                            });
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "justify",
                                    &mut canvas.justify,
                                    &[
                                        ("Start", blit_gui::layout::Justify::Start),
                                        ("Center", blit_gui::layout::Justify::Center),
                                        ("End", blit_gui::layout::Justify::End),
                                    ],
                                );
                            });
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "space",
                                    &mut canvas.justify,
                                    &[
                                        ("Between", blit_gui::layout::Justify::SpaceBetween),
                                        ("Around", blit_gui::layout::Justify::SpaceAround),
                                        ("Evenly", blit_gui::layout::Justify::SpaceEvenly),
                                    ],
                                );
                            });
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "align",
                                    &mut canvas.align,
                                    &[
                                        ("Start", Align::Start),
                                        ("Center", Align::Center),
                                        ("End", Align::End),
                                        ("Stretch", Align::Stretch),
                                    ],
                                );
                            });
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "sizing",
                                    &mut canvas.sizing,
                                    &[
                                        ("Fixed", ItemSizing::Fixed),
                                        ("Fit", ItemSizing::Fit),
                                        ("Grow", ItemSizing::Grow),
                                    ],
                                );
                            });
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "zoom",
                                    &mut canvas.zoom,
                                    &[("75%", 0.75), ("100%", 1.0), ("125%", 1.25)],
                                );
                            });
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "gap",
                                    &mut canvas.gap_steps,
                                    &[("0", 0), ("1", 1), ("2", 2), ("3", 3)],
                                );
                            });
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "padding",
                                    &mut canvas.padding_steps,
                                    &[("0", 0), ("1", 1), ("2", 2), ("3", 3)],
                                );
                            });
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "transition",
                                    &mut canvas.transitions,
                                    &[("On", true), ("Off", false)],
                                );
                            });
                            controls.child(flex::item()).insert(
                                Text::new("Drag the highlighted right edge, bottom edge, or corner. Layout changes preserve item identity and animate geometry.")
                                    .style(TextStyle {
                                        size: sz::MD,
                                        ..TextStyle::default()
                                    })
                                    .color(colors::TEXT_DIM)
                                    .options(blit_gui::text::TextOptions {
                                        wrap: blit_gui::text::TextWrap::Word,
                                        ..Default::default()
                                    }),
                            );
                        }),
                    );
                },
                |ui: Ui<'_>| {
                    let mut preview = ui.layout(flex::column().padding(Sides::all(sz::LG)).gap(sz::SM));
                    preview.insert(panel(colors::SURFACE));
                    preview
                        .child(flex::item().height(Sizing::fixed(sz::XXL)))
                        .insert(
                            Text::new("LIVE PREVIEW")
                                .style(TextStyle {
                                    size: sz::MD,
                                    ..TextStyle::default()
                                })
                                .color(colors::ACCENT),
                        );
                    preview.child(flex::item().grow()).build(|ui: Ui<'_>| {
                        let mut viewport = ui
                            .layout(single::layout().padding(Sides::all(sz::SM)))
                            .clip(BoundsClip);
                        viewport.insert(
                            Rectangle::new()
                                .background(colors::TRACK)
                                .radius(BorderRadius::uniform(sz::XS)),
                        );
                        let initial = (screen - sz::CANVAS_INITIAL_OFFSET)
                            .max(sz::CANVAS_INITIAL_MIN)
                            * sz::CANVAS_INITIAL_SCALE;
                        viewport.child(single::item()).build(
                            resize::new(
                                resize,
                                WidgetId::new("layout canvas"),
                                resize::Config::new(initial)
                                    .minimum(sz::CANVAS_MIN)
                                    .grip_size(Size::uniform(sz::MD)),
                                Canvas {
                                    config: preview_config,
                                    unit,
                                },
                                DesktopGrip,
                            ),
                        );
                    });
                },
            ),
        );
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ShadowKind {
    None,
    #[default]
    Outer,
    Inset,
}

struct StylesPage {
    shadow: ShadowKind,
    radius: f32,
    blur: f32,
    spread: f32,
    offset: (f32, f32),
    scroll: scroll_area::State,
    split: split::State,
}

impl Default for StylesPage {
    fn default() -> Self {
        Self {
            shadow: ShadowKind::Outer,
            radius: sz::LG,
            blur: sz::LG,
            spread: sz::BORDER_STRONG,
            offset: (0.0, sz::SM),
            scroll: scroll_area::State::default(),
            split: split::State::default(),
        }
    }
}

impl Widget<GuiContext> for &mut StylesPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let StylesPage {
            shadow,
            radius,
            blur,
            spread,
            offset,
            scroll: controls_scroll,
            split: split_state,
        } = self;
        let shadow_kind = *shadow;
        let preview_radius = *radius;
        let shadow_blur = *blur;
        let shadow_spread = *spread;
        let shadow_offset = *offset;
        let mut body = ui.layout(flex::row());
        body.child(flex::item().grow()).build(
            split(
                split_state,
                WidgetId::new("styles page split"),
                split::Config::new(sz::SIDEBAR).divider_extent(sz::LG),
                |ui: Ui<'_>| {
                    let mut sidebar = ui.layout(
                        flex::column()
                            .padding(Sides::all(sz::LG))
                            .gap(sz::SM),
                    );
                    sidebar.insert(panel(colors::SURFACE));
                    sidebar.child(flex::item()).insert(
                        Text::new("SHADOW ATOM")
                            .style(TextStyle {
                                size: sz::LG,
                                ..TextStyle::default()
                            })
                            .color(colors::ACCENT),
                    );
                    sidebar.child(flex::item().grow()).build(
                        scroll_area(
                            controls_scroll,
                            scroll_area::Config::new(),
                            |ui: Ui<'_>| {
                            let mut controls = ui.layout(flex::column().gap(sz::SM));
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "shadow",
                                    shadow,
                                    &[
                                        ("Outer", ShadowKind::Outer),
                                        ("Inset", ShadowKind::Inset),
                                        ("None", ShadowKind::None),
                                    ],
                                );
                            });
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "radius",
                                    radius,
                                    &[("0", 0.0), ("18", sz::LG), ("32", sz::XXL)],
                                );
                            });
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "blur",
                                    blur,
                                    &[("0", 0.0), ("8", sz::XS), ("18", sz::LG)],
                                );
                            });
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "spread",
                                    spread,
                                    &[
                                        ("0", 0.0),
                                        ("2", sz::BORDER_STRONG),
                                        ("8", sz::XS),
                                    ],
                                );
                            });
                            controls.child(flex::item()).build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "offset",
                                    offset,
                                    &[
                                        ("None", (0.0, 0.0)),
                                        ("Down", (0.0, sz::SM)),
                                        ("Side", (sz::SM, sz::SM)),
                                    ],
                                );
                            });
                            controls.child(flex::item()).insert(
                                Text::new("Shadow is an independent atom inserted on the card node. Insert it before the rectangle for an outer shadow or after it for an inset shadow.")
                                    .style(TextStyle {
                                        size: sz::MD,
                                        ..TextStyle::default()
                                    })
                                    .color(colors::TEXT_DIM)
                                    .options(blit_gui::text::TextOptions {
                                        wrap: blit_gui::text::TextWrap::Word,
                                        ..Default::default()
                                    }),
                            );
                        }),
                    );
                },
                |ui: Ui<'_>| {
                    let mut preview = ui.layout(flex::column().padding(Sides::all(sz::LG)).gap(sz::SM));
                    preview.insert(panel(colors::SURFACE));
                    preview
                        .child(flex::item().height(Sizing::fixed(sz::XXL)))
                        .insert(
                            Text::new("LIVE STYLE PREVIEW")
                                .style(TextStyle {
                                    size: sz::MD,
                                    ..TextStyle::default()
                                })
                                .color(colors::ACCENT),
                        );
                    preview.child(flex::item().grow()).build(|ui: Ui<'_>| {
                        let mut stage = ui.layout(
                            flex::row()
                                .padding(Sides::all(sz::XXXL))
                                .align(Align::Center)
                                .justify(blit_gui::layout::Justify::Center),
                        );
                        stage
                            .child(flex::item().fixed(sz::CARD_WIDTH, sz::CARD_HEIGHT))
                            .build(|ui: Ui<'_>| {
                                let mut card = ui.layout(
                                    flex::column()
                                        .padding(Sides::all(sz::XXL))
                                        .gap(sz::SM)
                                        .align(Align::Center)
                                        .justify(blit_gui::layout::Justify::Center),
                                );
                                let border_radius = BorderRadius::uniform(preview_radius);
                                let shadow = Shadow::new(colors::SHADOW)
                                    .radius(border_radius)
                                    .offset(shadow_offset.0, shadow_offset.1)
                                    .blur(shadow_blur)
                                    .spread(shadow_spread)
                                    .inset(shadow_kind == ShadowKind::Inset);
                                if shadow_kind == ShadowKind::Outer {
                                    card.insert(shadow);
                                }
                                card.insert(
                                    Rectangle::new()
                                        .background(colors::SURFACE_HIGH)
                                        .border(Border::solid(sz::BORDER, colors::CANVAS_BORDER))
                                        .radius(border_radius),
                                );
                                if shadow_kind == ShadowKind::Inset {
                                    card.insert(shadow);
                                }
                                card.child(flex::item()).insert(
                                    Text::new("SHADOW / ANY NODE")
                                        .style(TextStyle {
                                            size: sz::XL,
                                            ..TextStyle::default()
                                        })
                                        .color(colors::TEXT),
                                );
                                card.child(flex::item()).insert(
                                    Text::new("outer and inset shadows share the node's resolved bounds")
                                        .style(TextStyle {
                                            size: sz::MD,
                                            ..TextStyle::default()
                                        })
                                        .color(colors::TEXT_MUTED),
                                );
                            });
                    });
                },
            ),
        );
    }
}

struct ScrollPage {
    axis: Axis,
    state: scroll_area::State,
}

impl Default for ScrollPage {
    fn default() -> Self {
        Self {
            axis: Axis::Vertical,
            state: scroll_area::State::default(),
        }
    }
}

impl Widget<GuiContext> for &mut ScrollPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let ScrollPage {
            axis: scroll_axis,
            state: scroll,
        } = self;
        let mut section = ui.layout(flex::column().padding(Sides::all(sz::LG)).gap(sz::SM));
        section.insert(panel(colors::SURFACE));
        {
            let mut header = section
                .child(flex::item())
                .layout(flex::row().gap(sz::XS).align(Align::Center));
            header.child(flex::item().grow()).insert(
                Text::new("SCROLL AREA")
                    .style(TextStyle {
                        size: sz::MD,
                        ..TextStyle::default()
                    })
                    .color(colors::TEXT_MUTED),
            );
            for (axis, label) in [
                (Axis::Vertical, "VERTICAL"),
                (Axis::Horizontal, "HORIZONTAL"),
            ] {
                if header.child(flex::item()).build(Button::new(
                    WidgetId::new(("desktop scroll axis", label)),
                    label,
                    *scroll_axis == axis,
                )) {
                    *scroll_axis = axis;
                    *scroll = scroll_area::State::default();
                }
            }
        }
        let axis = *scroll_axis;
        let item_extent = match axis {
            Axis::Horizontal => sz::SCROLL_ITEM_WIDTH,
            Axis::Vertical => sz::SCROLL_ITEM_HEIGHT,
        };
        section.child(flex::item().grow()).build(scroll_list::new(
            scroll,
            scroll_list::Config::new(item_extent)
                .axis(axis)
                .gap(sz::XS)
                .behavior(scroll_behavior()),
            0_usize..100,
            move |ui, index| {
                let item = ITEMS[index % ITEMS.len()];
                let layout = match axis {
                    Axis::Horizontal => flex::column()
                        .align(Align::Center)
                        .justify(blit_gui::layout::Justify::Center),
                    Axis::Vertical => flex::row().padding(Sides::all(sz::XS)).align(Align::Center),
                };
                let background = if index.is_multiple_of(2) {
                    colors::CANVAS
                } else {
                    colors::SURFACE_HIGH
                };
                let mut tile = ui.layout(layout);
                tile.insert(
                    Rectangle::new()
                        .background(background)
                        .radius(BorderRadius::uniform(sz::XXS)),
                );
                tile.child(flex::item()).insert(
                    Text::new(item.label)
                        .style(TextStyle {
                            size: sz::MD,
                            ..TextStyle::default()
                        })
                        .color(colors::TEXT),
                );
            },
            scrollbar,
        ));
    }
}

fn split<'a>(
    state: &'a mut split::State,
    id: WidgetId,
    config: split::Config,
    leading: impl Widget<GuiContext> + 'a,
    trailing: impl Widget<GuiContext> + 'a,
) -> impl Widget<GuiContext> + 'a {
    split::new(
        state,
        id,
        config,
        |axis, interaction| {
            move |ui: Ui<'_>| {
                let active = interaction.hovered || interaction.dragging;
                let marker = match axis {
                    Axis::Horizontal => Size::new(sz::BORDER_STRONG, sz::XXXL),
                    Axis::Vertical => Size::new(sz::XXXL, sz::BORDER_STRONG),
                };
                let mut divider = ui.layout(
                    flex::row()
                        .align(Align::Center)
                        .justify(blit_gui::layout::Justify::Center),
                );
                divider
                    .child(flex::item().fixed(marker.width, marker.height))
                    .insert(
                        Rectangle::new()
                            .background(if active {
                                colors::ACCENT
                            } else {
                                colors::BORDER
                            })
                            .radius(BorderRadius::uniform(sz::BORDER)),
                    );
            }
        },
        leading,
        trailing,
    )
}

struct DesktopGrip(resize::Grip);

impl Widget<GuiContext> for DesktopGrip {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let marker = match self.0.edge {
            resize::Edge::Right => Size::new(sz::XXS, sz::XXXXL),
            resize::Edge::Bottom => Size::new(sz::XXXXL, sz::XXS),
            resize::Edge::Corner => Size::uniform(sz::XS),
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
        let mut grip = ui.layout(
            flex::row()
                .align(Align::Center)
                .justify(blit_gui::layout::Justify::Center),
        );
        grip.child(flex::item().fixed(marker.width, marker.height))
            .insert(
                Rectangle::new()
                    .background(color)
                    .radius(BorderRadius::uniform(marker.width.min(marker.height) / 2.0)),
            );
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

impl Widget<GuiContext> for Button<'_> {
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
    let background = if interaction.active {
        colors::ACCENT_DARK
    } else if selected {
        colors::SELECTED
    } else if interaction.hovered {
        colors::SURFACE_HIGH
    } else {
        colors::TRACK
    };
    let border = if selected {
        colors::ACCENT
    } else {
        colors::BORDER
    };
    let mut button = ui.layout(flex::row().padding(Sides::xy(sz::SM, sz::XS)));
    button.insert(
        Rectangle::new()
            .background(background)
            .border(Border::solid(sz::BORDER, border))
            .radius(BorderRadius::uniform(sz::XXS)),
    );
    button.child(flex::item()).insert(
        Text::new(label)
            .style(TextStyle {
                size: sz::MD,
                ..TextStyle::default()
            })
            .color(colors::TEXT),
    );
}

fn choices<T: Copy + PartialEq>(ui: Ui<'_>, label: &str, selected: &mut T, options: &[(&str, T)]) {
    let mut group = ui.layout(flex::column().gap(sz::XXS));
    group.child(flex::item()).insert(
        Text::new(label)
            .style(TextStyle {
                size: sz::MD,
                ..TextStyle::default()
            })
            .color(colors::TEXT_MUTED),
    );
    group.child(flex::item()).build(|ui: Ui<'_>| {
        let mut values = ui.layout(wrap::horizontal().item_gap(sz::XXS).run_gap(sz::XXS));
        for (index, &(option, value)) in options.iter().enumerate() {
            let clicked = values.child(wrap::item()).build(Button::new(
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

impl Widget<GuiContext> for Canvas {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let background = Rectangle::new()
            .background(colors::CANVAS)
            .border(Border::solid(sz::BORDER_STRONG, colors::CANVAS_BORDER))
            .radius(BorderRadius::uniform(sz::XS));
        let ui = ui.widget_id(WidgetId::new("desktop canvas"));
        match self.config.layout {
            CanvasLayout::Flex => {
                let mut canvas = ui
                    .layout(
                        flex::layout(self.config.axis)
                            .padding(self.config.padding(self.unit))
                            .gap(self.config.gap(self.config.axis, self.unit))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .clip(BoundsClip);
                canvas.insert(background);
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    let (width, height) = self.config.item_sizing(index, self.unit);
                    canvas
                        .child(flex::item().width(width).height(height))
                        .build(|ui: Ui<'_>| {
                            canvas_item(ui, index, spec, self.config);
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
                        wrap::layout(self.config.axis)
                            .padding(self.config.padding(self.unit))
                            .item_gap(self.config.gap(self.config.axis, self.unit))
                            .run_gap(self.config.gap(cross, self.unit))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .clip(BoundsClip);
                canvas.insert(background);
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    let (width, height) = self.config.item_sizing(index, self.unit);
                    canvas
                        .child(wrap::item().width(width).height(height))
                        .build(|ui: Ui<'_>| {
                            canvas_item(ui, index, spec, self.config);
                        });
                }
            }
            CanvasLayout::Grid => {
                let grid = grid::columns(5)
                    .spanning()
                    .padding(self.config.padding(self.unit))
                    .column_gap(self.config.gap(Axis::Horizontal, self.unit))
                    .row_gap(self.config.gap(Axis::Vertical, self.unit));
                let mut canvas = ui.layout(grid).clip(BoundsClip);
                canvas.insert(background);
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    canvas
                        .child(
                            grid::item()
                                .row_span(spec.rows)
                                .column_span(spec.columns)
                                .preferred_height(5.0 * self.unit.height * self.config.zoom),
                        )
                        .build(|ui: Ui<'_>| {
                            canvas_item(ui, index, spec, self.config);
                        });
                }
            }
        }
    }
}

fn canvas_item(ui: Ui<'_>, index: usize, spec: blit_demo::ItemSpec, config: CanvasConfig) {
    let mut item = ui.layout(
        flex::column()
            .align(Align::Center)
            .justify(blit_gui::layout::Justify::Center),
    );
    item.insert(
        Rectangle::new()
            .background(colors::ITEMS[index])
            .radius(BorderRadius::uniform(sz::XXS)),
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
    item.child(flex::item()).insert(
        Text::new(spec.label)
            .style(TextStyle {
                size: sz::MD * config.zoom,
                ..TextStyle::default()
            })
            .color(Color::WHITE),
    );
    if let Some(anchor) = spec.badge {
        item.absolute(
            Absolute::attach(anchor, Anchor::Center)
                .width(Sizing::fixed(sz::BADGE_WIDTH * config.zoom))
                .height(Sizing::fixed(sz::LG * config.zoom)),
        )
        .parent(WidgetId::new("desktop canvas"))
        .z_index(1)
        .build(|ui: Ui<'_>| {
            let mut badge = ui.layout(
                flex::row()
                    .align(Align::Center)
                    .justify(blit_gui::layout::Justify::Center),
            );
            badge.insert(
                Rectangle::new()
                    .background(colors::BACKGROUND)
                    .border(Border::solid(sz::BORDER, Color::WHITE))
                    .radius(BorderRadius::uniform(sz::XXS)),
            );
            badge.child(flex::item()).insert(
                Text::new("ABS")
                    .style(TextStyle {
                        size: (sz::XS * config.zoom).max(sz::XS),
                        ..TextStyle::default()
                    })
                    .color(Color::WHITE),
            );
        });
    }
}

fn scroll_area<'a, C>(
    state: &'a mut scroll_area::State,
    config: scroll_area::Config,
    content: C,
) -> impl Widget<GuiContext> + 'a
where
    C: Widget<GuiContext> + 'a,
{
    scroll_area::new(
        state,
        config.behavior(scroll_behavior()),
        content,
        scrollbar,
    )
}

fn scroll_behavior() -> scroll_area::Behavior {
    scroll_area::Behavior::new()
        .scroll_speed(2.0)
        .inertia_friction(3.0)
        .scrollbar_thickness(sz::XS)
        .minimum_thumb_extent(sz::XXL)
}

fn scrollbar(active: bool) -> (Option<Rectangle>, Option<Rectangle>) {
    (
        Some(
            Rectangle::new()
                .background(colors::SCROLL_TRACK)
                .radius(BorderRadius::uniform(sz::XXS)),
        ),
        Some(
            Rectangle::new()
                .background(if active {
                    colors::TEXT_DIM
                } else {
                    colors::BORDER
                })
                .radius(BorderRadius::uniform(sz::XXS)),
        ),
    )
}

fn panel(background: Color) -> Rectangle {
    Rectangle::new()
        .background(background)
        .border(Border::solid(sz::BORDER, colors::BORDER))
        .radius(BorderRadius::uniform(sz::SM))
}

mod sz {
    use blit::Size;

    pub const BORDER: f32 = 1.0;
    pub const BORDER_STRONG: f32 = 2.0;

    pub const XXS: f32 = 4.0;
    pub const XS: f32 = 8.0;
    pub const SM: f32 = 12.0;
    pub const MD: f32 = 14.0;
    pub const LG: f32 = 18.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
    pub const XXXL: f32 = 48.0;
    pub const XXXXL: f32 = 64.0;

    pub const SIDEBAR: f32 = 360.0;
    pub const SCROLL_ITEM_WIDTH: f32 = 110.0;
    pub const SCROLL_ITEM_HEIGHT: f32 = 40.0;
    pub const CARD_WIDTH: f32 = 420.0;
    pub const CARD_HEIGHT: f32 = 240.0;
    pub const BADGE_WIDTH: f32 = 36.0;

    pub const CANVAS_INITIAL_SCALE: f32 = 0.8;
    pub const CANVAS_INITIAL_OFFSET: Size = Size::new(430.0, 150.0);
    pub const CANVAS_INITIAL_MIN: Size = Size::new(280.0, 220.0);
    pub const CANVAS_MIN: Size = Size::new(240.0, 180.0);
}

mod colors {
    use blit_gui::color::Color;

    pub const BACKGROUND: Color = Color::from_rgba8(12, 18, 29, 255);
    pub const SURFACE: Color = Color::from_rgba8(20, 29, 45, 255);
    pub const SURFACE_HIGH: Color = Color::from_rgba8(38, 53, 77, 255);
    pub const TRACK: Color = Color::from_rgba8(9, 15, 25, 255);
    pub const SCROLL_TRACK: Color = Color::from_rgba8(9, 15, 25, 96);
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
    pub const SHADOW: Color = Color::from_rgba8(0, 0, 0, 180);
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
