use std::{cell::Cell, time::Duration};

use blit::{
    Animation, Atom, AtomCx, Axis, Constraints, Easing, Id, LogicalRect, Motion, NodeTarget, Sides, Size, Tween, Widget,
};
use blit_demo::{CanvasConfig, CanvasLayout, ITEMS, ItemSizing, SpringDrag, Transition, select};
#[cfg(not(feature = "gpu"))]
use blit_desktop::cpu;
#[cfg(feature = "gpu")]
use blit_desktop::gpu;
use blit_desktop::{Application, Config, EventLoopProxy, Scope};
use blit_gui::{
    clip::BoundsClip,
    color::Color,
    mesh::{Mesh, MeshVertex},
    rectangle::Rectangle,
    style::{Border, BorderRadius, GradientStop, LinearGradient},
    text::{
        FontId, HorizontalAlign, RichText, Span, SpanStyle, Text, TextOptions, TextOverflow, TextStyle, TextWrap,
        VerticalAlign,
        system::{FontStack, TextConfig, TextSystem},
    },
};
use blit_gui_widgets::{
    interact,
    layout::{Align, Justify, Pad, Sizing, absolute, absolute::Anchor, flex, grid, wrap},
    performance::{Performance, PerformanceState},
    popover::{Popover, PopoverClose, PopoverState},
    resize::{Resize, ResizeEdge, ResizeGrip, ResizeState},
    scroll::{
        ScrollState,
        area::ScrollArea,
        uniform_list::UniformList,
        virtual_list::{VirtualList, VirtualListState},
    },
    split::{Split, SplitState},
    text_input::{TextInput, TextInputState},
};
use blit_text::{Family, FontData, FontStyle};

fn main() {
    let fonts = std::fs::read_dir(std::path::Path::new(env!("BLIT_TEST_FONT")).with_file_name(""))
        .unwrap()
        .filter_map(|entry| std::fs::read(entry.ok()?.path()).ok())
        .map(|data| FontData::Shared(data.into()))
        .collect::<Vec<_>>();
    let text = TextSystem::new(
        TextConfig {
            fonts: vec![FontStack {
                id: FontId::default(),
                families: vec![
                    Family::embedded(fonts),
                    Family::named("Noto Color Emoji"),
                    Family::named("Apple Color Emoji"),
                ],
            }],
            text_cache_capacity: 1024 * 1024,
            layout_cache_capacity: 2 * 1024 * 1024,
        },
        blit_text_parley::Backend::new(),
    )
    .unwrap();
    #[cfg(not(feature = "gpu"))]
    let graphics = cpu::Backend::new(text, cpu::Config::default());
    #[cfg(feature = "gpu")]
    let graphics = gpu::Backend::new(text, gpu::Config::default());
    blit_desktop::run::<App>(Config {
        title: "blit / demo".into(),
        width: 1120,
        height: 800,
        graphics,
    })
    .unwrap();
}

#[cfg(not(feature = "gpu"))]
type DesktopContext = blit_desktop::DesktopContext<cpu::Backend>;
#[cfg(feature = "gpu")]
type DesktopContext = blit_desktop::DesktopContext<gpu::Backend>;
type Ui<'a> = blit::Ui<'a, DesktopContext>;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Page {
    Layout,
    Text,
    Input,
    Scroll,
    Animation,
    #[default]
    Graphics,
}

struct App {
    page: Page,
    layout: LayoutPage,
    text: TextPage,
    input: InputPage,
    scroll: ScrollPage,
    animation: AnimationPage,
    graphics: graphics::View,
    settings: PopoverState,
    performance: PerformanceState,
    show_performance: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            page: Page::default(),
            layout: LayoutPage::default(),
            text: TextPage::default(),
            input: InputPage::default(),
            scroll: ScrollPage::default(),
            animation: AnimationPage::default(),
            graphics: graphics::View::default(),
            settings: PopoverState::new(),
            performance: PerformanceState::default(),
            show_performance: true,
        }
    }
}

impl Application for App {
    type Input = ();
    #[cfg(not(feature = "gpu"))]
    type Graphics = cpu::Backend;
    #[cfg(feature = "gpu")]
    type Graphics = gpu::Backend;

    fn new(_: EventLoopProxy<Self::Input>, _: Scope<Self>, context: &mut DesktopContext) -> Self {
        context.profiler.set_enabled(true);
        Self::default()
    }

    fn input(&mut self, _: Self::Input) {}

    fn build(&mut self, ui: Ui<'_>) {
        let mut root = ui.layout(flex::column().gap(sz::SM));
        root.insert(Rectangle::new().background(colors::BACKGROUND));
        root.insert(Pad::new(sz::XL));
        {
            let mut masthead = root.child().layout(flex::row().gap(3.0).align(Align::Start));
            masthead
                .child()
                .insert(Text::new("blit.").style(TextStyle::new().size(40.0).weight(700).color(colors::TEXT)));
            #[cfg(not(feature = "gpu"))]
            let backend = "CPU";
            #[cfg(feature = "gpu")]
            let backend = "GPU";
            let mut badge = masthead.child().layout(flex::column());
            badge.insert(Pad::new(Sides::new().top(6.0)));
            badge
                .child()
                .insert(Text::new(backend).style(TextStyle::new().size(sz::SM).color(colors::ACCENT)));
        }
        {
            let mut header = root.child().layout(flex::row().gap(sz::XS).align(Align::Center));
            header.insert(Rectangle::new().background(colors::SURFACE).border(Border::gradient(
                sz::BORDER,
                LinearGradient::new(&colors::RIM).angle(90.0),
            )));
            header.insert(Pad::new(sz::XXS));
            header.child().build(select(
                &mut self.page,
                &[
                    ("01 layout", Page::Layout),
                    ("02 type", Page::Text),
                    ("03 input", Page::Input),
                    ("04 scroll", Page::Scroll),
                    ("05 motion", Page::Animation),
                    ("06 graphics", Page::Graphics),
                ],
                flex::row().gap(sz::XS),
                draw_button,
            ));
            header.child().item(flex::item().grow()).insert(());
            let reset = header.child().build(
                Popover::new(&mut self.settings)
                    .target_anchor(Anchor::BottomRight)
                    .child_anchor(Anchor::TopRight)
                    .y(sz::XXS)
                    .open_on_hover(true)
                    .close(PopoverClose::Exit)
                    .trigger(|ui, press, open| {
                        draw_button(ui, "settings", open, press);
                    })
                    .content(|ui: Ui<'_>| {
                        let mut popup = ui.layout(flex::column().gap(sz::SM));
                        popup.insert(
                            Rectangle::new()
                                .background(colors::SURFACE)
                                .border(Border::solid(sz::BORDER, colors::ACCENT))
                                .radius(BorderRadius::uniform(sz::XS)),
                        );
                        popup.insert(Pad::new(sz::MD));
                        if popup
                            .child()
                            .build(Button::new("Show performance", self.show_performance))
                        {
                            self.show_performance = !self.show_performance;
                        }
                        popup.child().build(Button::new("Reset", false))
                    }),
            );
            if reset.unwrap_or(false) {
                *self = Self::default();
            }
        }
        match self.page {
            Page::Layout => root.child().item(flex::item().grow()).build(&mut self.layout),
            Page::Text => root.child().item(flex::item().grow()).build(&mut self.text),
            Page::Input => root.child().item(flex::item().grow()).build(&mut self.input),
            Page::Scroll => root.child().item(flex::item().grow()).build(&mut self.scroll),
            Page::Animation => root.child().item(flex::item().grow()).build(&mut self.animation),
            Page::Graphics => root.child().item(flex::item().grow()).build(&mut self.graphics),
        };
        root.context_mut().profiler.set_enabled(self.show_performance);
        if self.show_performance {
            root.child()
                .place(
                    NodeTarget::Root,
                    absolute::place()
                        .target_anchor(Anchor::BottomRight)
                        .child_anchor(Anchor::BottomRight)
                        .x(-sz::LG)
                        .y(-sz::LG),
                )
                .layout(())
                .child()
                .build(
                    Performance::new(&mut self.performance)
                        .background(colors::SURFACE_HIGH)
                        .graph_background(colors::CANVAS)
                        .color(colors::TEXT)
                        .muted_color(colors::TEXT_DIM)
                        .accent(colors::ACCENT)
                        .border(Border::solid(1.0, colors::BORDER))
                        .radius(BorderRadius::uniform(2.0)),
                );
        }
    }
}

#[derive(Default)]
struct AnimationPage {
    drag: SpringDrag,
    show_drag: bool,
    right: bool,
    scroll: ScrollState,
    split: SplitState,
}

impl Widget<DesktopContext> for &mut AnimationPage {
    type Response = ();

    fn build(self, mut ui: Ui<'_>) {
        let AnimationPage {
            drag,
            show_drag,
            right,
            scroll: controls_scroll,
            split: split_state,
        } = self;
        let id = Id::new("spring card");
        let position = drag.update(&mut ui, id);
        let active = drag.active;
        let settings = Cell::new((*show_drag, drag.spring));
        let mut body = ui.layout(flex::row());
        body.child()
            .item(flex::item().grow())
            .id(Id::new("animation page split"))
            .build(split(
                split_state,
                sz::SIDEBAR,
                |ui: Ui<'_>| {
                    let mut sidebar = ui.layout(flex::column().gap(sz::XS)).clip(BoundsClip);
                    sidebar.insert(panel(colors::SURFACE));
                    sidebar.insert(Pad::new(sz::LG));
                    sidebar
                        .child()
                        .insert(Text::new("PARAMETERS").style(TextStyle::new().size(sz::MD).color(colors::ACCENT)));
                    sidebar
                        .child()
                        .item(flex::item().grow())
                        .build(scroll_area(controls_scroll, |ui: Ui<'_>| {
                            let mut controls = ui.layout(flex::column().gap(sz::LG));
                            controls.child().build(|ui: Ui<'_>| {
                                choices(ui, "mode", show_drag, &[("retarget", false), ("drag", true)]);
                            });
                            controls.child().build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "bounce",
                                    &mut drag.spring.bounce,
                                    &[("0", 0.0), ("0.25", 0.25), ("0.5", 0.5)],
                                );
                            });
                            controls.child().build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "duration",
                                    &mut drag.spring.duration,
                                    &[
                                        ("350ms", Duration::from_millis(350)),
                                        ("650ms", Duration::from_millis(650)),
                                        ("1000ms", Duration::from_millis(1000)),
                                        ("2000ms", Duration::from_millis(2000)),
                                    ],
                                );
                            });
                        }));
                    settings.set((*show_drag, drag.spring));
                },
                |ui: Ui<'_>| {
                    let (show_drag, spring) = settings.get();
                    let mut preview = ui.layout(flex::column().gap(sz::SM)).clip(BoundsClip);
                    preview.insert(panel(colors::SURFACE));
                    preview.insert(Pad::new(sz::LG));
                    preview
                        .child()
                        .insert(Text::new("LIVE VIEW").style(TextStyle::new().size(sz::MD).color(colors::ACCENT)));
                    if !show_drag {
                        {
                            let mut action = preview
                                .child()
                                .item(flex::item().width(Sizing::grow()))
                                .layout(flex::row().justify(Justify::Center));
                            if action
                                .child()
                                .build(Button::new(if *right { "move left" } else { "move right" }, false))
                            {
                                *right = !*right;
                            }
                        }
                        let tween = Tween::new(spring.duration, Easing::EASE_IN_OUT);
                        for (index, (label, animation, restart)) in [
                            ("tween / carry", Animation::Tween(tween), false),
                            ("tween / restart", Animation::Tween(tween), true),
                            ("spring / carry", Animation::Spring(spring), false),
                            ("spring / restart", Animation::Spring(spring), true),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let mut row = preview
                                .child()
                                .item(flex::item().grow())
                                .layout(flex::column().gap(sz::XXS));
                            row.child().insert(
                                Text::new(label).style(TextStyle::new().size(sz::MD).color(colors::TEXT_MUTED)),
                            );
                            let mut lane = row.child().item(flex::item().grow()).layout(()).clip(BoundsClip);
                            lane.insert(panel(colors::CANVAS));
                            let mut track = lane
                                .child()
                                .place(
                                    NodeTarget::Parent,
                                    absolute::place()
                                        .target_anchor(Anchor::Center)
                                        .child_anchor(Anchor::Center)
                                        .width(absolute::Sizing::percent(0.65))
                                        .height(absolute::Sizing::full()),
                                )
                                .layout(());
                            let marker = absolute::place()
                                .target_anchor(if *right { Anchor::Right } else { Anchor::Left })
                                .child_anchor(Anchor::Center)
                                .width(absolute::Sizing::fixed(20.0))
                                .height(absolute::Sizing::fixed(20.0));
                            track.child().place(NodeTarget::Parent, marker).insert(
                                Rectangle::new()
                                    .border(Border::solid(sz::BORDER, colors::CANVAS_BORDER))
                                    .radius(BorderRadius::uniform(sz::XXS)),
                            );
                            track
                                .child()
                                .id(Id::new(("retarget", index)))
                                .place(NodeTarget::Parent, marker)
                                .motion(Motion::new(animation).x().restart(restart))
                                .insert(
                                    Rectangle::new()
                                        .background(colors::ACCENT)
                                        .radius(BorderRadius::uniform(sz::XXS)),
                                );
                        }
                        return;
                    }
                    let mut canvas = preview.child().item(flex::item().grow()).layout(()).clip(BoundsClip);
                    canvas.insert(panel(colors::CANVAS));
                    let content = flex::column().align(Align::Center).justify(Justify::Center);
                    let card = absolute::place()
                        .target_anchor(Anchor::Center)
                        .child_anchor(Anchor::Center)
                        .width(absolute::Sizing::fixed(128.0))
                        .height(absolute::Sizing::fixed(96.0));
                    canvas
                        .child()
                        .id(Id::new((id, "home")))
                        .place(NodeTarget::Parent, card)
                        .layout(content)
                        .insert(
                            Rectangle::new()
                                .border(Border::solid(sz::BORDER, colors::CANVAS_BORDER))
                                .radius(BorderRadius::uniform(sz::SM)),
                        );
                    let mut handle = canvas
                        .child()
                        .id(id)
                        .place(NodeTarget::Parent, card.x(position.x).y(position.y))
                        .motion(Motion::new(spring).position())
                        .layout(content);
                    handle.insert(
                        Rectangle::new()
                            .background(if active { colors::ACCENT_DARK } else { colors::ACCENT })
                            .radius(BorderRadius::uniform(sz::SM)),
                    );
                    handle
                        .child()
                        .insert(Text::new("drag me").style(TextStyle::new().size(sz::LG).color(colors::BACKGROUND)));
                },
            ));
    }
}

struct TextPage {
    text: String,
    spans: Vec<Span<'static>>,
    resize: ResizeState,
    wrap: TextWrap,
    overflow: TextOverflow,
    horizontal: HorizontalAlign,
    vertical: VerticalAlign,
    max_lines: Option<u16>,
    split: SplitState,
}

impl Default for TextPage {
    fn default() -> Self {
        let mut text = String::new();
        let mut spans = Vec::new();
        for (value, style) in [
            (
                "Type specimen / 02\n",
                SpanStyle::new()
                    .size(sz::XXL)
                    .weight(700)
                    .style(FontStyle::Italic)
                    .color(colors::ACCENT),
            ),
            ("😀 🚀 🌈 ❤️ 👨‍👩‍👧‍👦 🇺🇸 1️⃣\n", SpanStyle::new().size(sz::XXL)),
            ("Text ©  Emoji ©️\n\n", SpanStyle::new().color(colors::TEXT)),
            ("One paragraph can mix inherited body text with ", SpanStyle::new()),
            ("large type", SpanStyle::new().size(sz::XL).color(colors::TEXT)),
            (", ", SpanStyle::new()),
            ("bold", SpanStyle::new().weight(700).color(colors::TEXT)),
            (", ", SpanStyle::new()),
            ("italic", SpanStyle::new().style(FontStyle::Italic).color(colors::TEXT)),
            (", ", SpanStyle::new()),
            (
                "oblique",
                SpanStyle::new().style(FontStyle::Oblique).color(colors::TEXT),
            ),
            (", and ", SpanStyle::new()),
            ("small details", SpanStyle::new().size(sz::SM).color(colors::TEXT_MUTED)),
            (
                ". Every styled span participates in the same wrapping, alignment, measurement, clipping, and ellipsis behavior. Resize the panel to watch the whole paragraph reflow.",
                SpanStyle::new(),
            ),
        ] {
            let start = text.len();
            text.push_str(value);
            spans.push(Span::new(start..text.len()).style(style));
        }

        Self {
            text,
            spans,
            resize: ResizeState::default(),
            wrap: TextWrap::Word,
            overflow: TextOverflow::Clip,
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
            max_lines: None,
            split: SplitState::default(),
        }
    }
}

impl Widget<DesktopContext> for &mut TextPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let TextPage {
            text,
            spans,
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
        body.child()
            .item(flex::item().grow())
            .id(Id::new("text page split"))
            .build(split(
                split_state,
                sz::SIDEBAR,
                |ui: Ui<'_>| {
                    let mut controls = ui.layout(flex::column().gap(sz::LG));
                    controls.insert(panel(colors::SURFACE));
                    controls.insert(Pad::new(sz::LG));
                    controls
                        .child()
                        .insert(Text::new("PARAMETERS").style(TextStyle::new().size(sz::MD).color(colors::ACCENT)));
                    controls.child().build(|ui: Ui<'_>| {
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
                    controls.child().build(|ui: Ui<'_>| {
                        choices(
                            ui,
                            "overflow",
                            overflow,
                            &[("Clip", TextOverflow::Clip), ("Ellipsis", TextOverflow::Ellipsis)],
                        );
                    });
                    controls.child().build(|ui: Ui<'_>| {
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
                    controls.child().build(|ui: Ui<'_>| {
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
                    controls.child().build(|ui: Ui<'_>| {
                        choices(
                            ui,
                            "max lines",
                            max_lines,
                            &[("All", None), ("3", Some(3)), ("6", Some(6))],
                        );
                    });
                },
                |ui: Ui<'_>| {
                    let mut preview = ui.layout(flex::column().gap(sz::SM));
                    preview.insert(panel(colors::SURFACE));
                    preview.insert(Pad::new(sz::LG));
                    preview
                        .child()
                        .insert(Text::new("text").style(TextStyle::new().size(sz::MD).color(colors::ACCENT)));
                    preview.child().item(flex::item().grow()).build(|ui: Ui<'_>| {
                        let mut viewport = ui.layout(flex::column()).clip(BoundsClip);
                        viewport.insert(
                            Rectangle::new()
                                .background(colors::TRACK)
                                .radius(BorderRadius::uniform(sz::XS)),
                        );
                        viewport.insert(Pad::new(sz::SM));
                        viewport.child().id(Id::new("rich text preview")).build(
                            Resize::new(resize)
                                .initial(Size::new(560.0, 360.0))
                                .minimum(Size::new(260.0, 160.0))
                                .maximum(screen)
                                .grip_size(Size::uniform(sz::MD))
                                .content(|ui: Ui<'_>| {
                                    let mut paragraph = ui.layout(()).clip(BoundsClip);
                                    paragraph.insert(
                                        Rectangle::new()
                                            .background(colors::CANVAS)
                                            .border(Border::solid(sz::BORDER, colors::CANVAS_BORDER))
                                            .radius(BorderRadius::uniform(sz::XS)),
                                    );
                                    paragraph.insert(Pad::new(sz::LG));
                                    paragraph.child().insert(
                                        RichText::new(text, spans)
                                            .style(TextStyle {
                                                size: sz::LG,
                                                color: colors::TEXT_MUTED,
                                                ..TextStyle::default()
                                            })
                                            .options(options),
                                    );
                                })
                                .grip(draw_grip),
                        );
                    });
                },
            ));
    }
}

#[derive(Default)]
struct InputPage {
    value: String,
    state: TextInputState,
}

impl Widget<DesktopContext> for &mut InputPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let mut body = ui.layout(flex::column().gap(sz::SM));
        body.insert(panel(colors::SURFACE));
        body.insert(Pad::new(sz::LG));
        body.child()
            .insert(Text::new("input").style(TextStyle::new().size(sz::MD).color(colors::TEXT_MUTED)));
        body.child().build(|ui: Ui<'_>| {
            let mut field = ui
                .constrain(Constraints::tight(Size::new(420.0, 40.0)))
                .id(Id::new("desktop text input"));
            let focused = field.interact(interact::focus());
            let hovered = field.interact(interact::press()).hovered;
            let border = if focused {
                Border::solid(sz::BORDER, colors::ACCENT_DARK)
            } else if hovered {
                Border::solid(sz::BORDER, colors::CANVAS_BORDER)
            } else {
                Border::solid(sz::BORDER, colors::BORDER)
            };
            field.insert(
                Rectangle::new()
                    .background(LinearGradient::new(&colors::INPUT).angle(90.0))
                    .border(border)
                    .radius(BorderRadius::uniform(6.0)),
            );
            field.build(
                TextInput::new(&mut self.state, &mut self.value)
                    .style(TextStyle {
                        size: sz::MD,
                        color: colors::TEXT,
                        ..TextStyle::default()
                    })
                    .padding(Sides::xy(sz::SM, 10.0))
                    .placeholder("Type here")
                    .placeholder_color(colors::TEXT_MUTED)
                    .selection_background(colors::ACCENT_DARK)
                    .cursor_background(colors::ACCENT),
            )
        });
        body.child().insert(Text::new("esc to unfocus").style(TextStyle {
            size: sz::SM,
            color: colors::TEXT_DIM,
            ..TextStyle::default()
        }));
    }
}

#[derive(Default)]
struct LayoutPage {
    canvas: CanvasConfig,
    resize: ResizeState,
    scroll: ScrollState,
    split: SplitState,
}

impl Widget<DesktopContext> for &mut LayoutPage {
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
        body.child()
            .item(flex::item().grow())
            .id(Id::new("layout page split"))
            .build(split(
                split_state,
                sz::SIDEBAR,
                |ui: Ui<'_>| {
                    let mut sidebar = ui.layout(flex::column().gap(sz::XS));
                    sidebar.insert(panel(colors::SURFACE));
                    sidebar.insert(Pad::new(sz::LG));
                    sidebar
                        .child()
                        .insert(Text::new("PARAMETERS").style(TextStyle::new().size(sz::MD).color(colors::ACCENT)));
                    sidebar
                        .child()
                        .item(flex::item().grow())
                        .build(scroll_area(controls_scroll, |ui: Ui<'_>| {
                            let mut controls = ui.layout(flex::column().gap(sz::LG));
                            controls.child().build(|ui: Ui<'_>| {
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
                            if matches!(canvas.layout, CanvasLayout::Flex | CanvasLayout::Wrap) {
                                controls.child().build(|ui: Ui<'_>| {
                                    choices(
                                        ui,
                                        "axis",
                                        &mut canvas.axis,
                                        &[("Horizontal", Axis::Horizontal), ("Vertical", Axis::Vertical)],
                                    );
                                });
                                controls.child().build(|ui: Ui<'_>| {
                                    choices(
                                        ui,
                                        "justify",
                                        &mut canvas.justify,
                                        &[
                                            ("Start", Justify::Start),
                                            ("Center", Justify::Center),
                                            ("End", Justify::End),
                                            ("Between", Justify::SpaceBetween),
                                            ("Around", Justify::SpaceAround),
                                            ("Evenly", Justify::SpaceEvenly),
                                        ],
                                    );
                                });
                                controls.child().build(|ui: Ui<'_>| {
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
                                controls.child().build(|ui: Ui<'_>| {
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
                            }
                            controls.child().build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "zoom",
                                    &mut canvas.zoom,
                                    &[("75%", 0.75), ("100%", 1.0), ("125%", 1.25)],
                                );
                            });
                            controls.child().build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "gap",
                                    &mut canvas.gap_steps,
                                    &[("0", 0), ("1", 1), ("2", 2), ("3", 3)],
                                );
                            });
                            controls.child().build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "padding",
                                    &mut canvas.padding_steps,
                                    &[("0", 0), ("1", 1), ("2", 2), ("3", 3)],
                                );
                            });
                            controls.child().build(|ui: Ui<'_>| {
                                choices(
                                    ui,
                                    "transition",
                                    &mut canvas.transition,
                                    &[
                                        ("Off", Transition::Off),
                                        ("Spring", Transition::Spring),
                                        ("Tween", Transition::Tween),
                                    ],
                                );
                            });
                        }));
                },
                |ui: Ui<'_>| {
                    let mut preview = ui.layout(flex::column().gap(sz::SM));
                    preview.insert(panel(colors::SURFACE));
                    preview.insert(Pad::new(sz::LG));
                    preview
                        .child()
                        .insert(Text::new("LIVE VIEW").style(TextStyle::new().size(sz::MD).color(colors::ACCENT)));
                    preview.child().item(flex::item().grow()).build(|ui: Ui<'_>| {
                        let mut viewport = ui.layout(flex::column()).clip(BoundsClip);
                        viewport.insert(
                            Rectangle::new()
                                .background(colors::TRACK)
                                .radius(BorderRadius::uniform(sz::XS)),
                        );
                        viewport.insert(Pad::new(sz::SM));
                        let initial =
                            (screen - sz::CANVAS_INITIAL_OFFSET).max(sz::CANVAS_INITIAL_MIN) * sz::CANVAS_INITIAL_SCALE;
                        viewport.child().id(Id::new("layout canvas")).build(
                            Resize::new(resize)
                                .initial(initial)
                                .minimum(sz::CANVAS_MIN)
                                .grip_size(Size::uniform(sz::MD))
                                .content(Canvas {
                                    config: preview_config,
                                    unit,
                                })
                                .grip(draw_grip),
                        );
                    });
                },
            ));
    }
}

#[derive(Default)]
struct ScrollPage {
    mode: ScrollMode,
    uniform: ScrollState,
    measured: VirtualListState,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ScrollMode {
    Finite,
    #[default]
    Infinite,
    Horizontal,
}

impl Widget<DesktopContext> for &mut ScrollPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let ScrollPage {
            mode,
            uniform,
            measured,
        } = self;
        let mut section = ui.layout(flex::column().gap(sz::SM));
        section.insert(panel(colors::SURFACE));
        section.insert(Pad::new(sz::LG));
        {
            let mut header = section.child().layout(flex::column().gap(sz::SM));
            header
                .child()
                .insert(Text::new("scroll").style(TextStyle::new().size(sz::MD).color(colors::ACCENT)));
            if header.child().build(select(
                mode,
                &[
                    ("finite", ScrollMode::Finite),
                    ("infinite", ScrollMode::Infinite),
                    ("horizontal", ScrollMode::Horizontal),
                ],
                flex::row().gap(sz::XS),
                draw_button,
            )) {
                *uniform = ScrollState::default();
                *measured = VirtualListState::default();
            }
        }
        let horizontal = *mode == ScrollMode::Horizontal;
        let row = move |ui: Ui<'_>, index: usize| {
            let item = ITEMS[index % ITEMS.len()];
            let (layout, padding) = if horizontal {
                (flex::column().align(Align::Center).justify(Justify::Center), 0.0)
            } else {
                (
                    flex::row().gap(sz::SM).align(Align::Center),
                    sz::XS + (index % 5) as f32 * sz::XXS,
                )
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
            tile.insert(Pad::new(padding));
            tile.child()
                .insert(Text::new(format_args!("{index}")).style(TextStyle::new().color(colors::TEXT_MUTED)));
            tile.child().insert(Text::new(item.label).style(TextStyle {
                size: sz::MD,
                color: colors::TEXT,
                ..TextStyle::default()
            }));
        };
        if horizontal {
            section.child().item(flex::item().grow()).build(
                UniformList::new(uniform, 0_usize..100)
                    .item_extent(sz::SCROLL_ITEM_WIDTH)
                    .axis(Axis::Horizontal)
                    .gap(sz::XS)
                    .scroll_speed(2.0)
                    .inertia_friction(3.0)
                    .scrollbar_thickness(sz::XS)
                    .minimum_thumb_extent(sz::XXL)
                    .clip(BoundsClip)
                    .build_scrollbar(row, scroll_track, scroll_thumb),
            );
        } else {
            section.child().item(flex::item().grow()).build(
                VirtualList::new(measured)
                    .count((*mode == ScrollMode::Finite).then_some(100_000))
                    .scroll_speed(2.0)
                    .inertia_friction(3.0)
                    .scrollbar_thickness(sz::XS)
                    .minimum_thumb_extent(sz::XS)
                    .clip(BoundsClip)
                    .build_scrollbar(row, scroll_track, scroll_thumb),
            );
        }
    }
}

fn split(
    state: &mut SplitState,
    initial_extent: f32,
    leading: impl Widget<DesktopContext>,
    trailing: impl Widget<DesktopContext>,
) -> impl Widget<DesktopContext> {
    Split::new(state)
        .initial_extent(initial_extent)
        .divider_extent(sz::LG)
        .divider(|ui, divider| {
            let active = divider.hovered || divider.active;
            let marker = match divider.axis {
                Axis::Horizontal => Size::new(sz::BORDER_STRONG, sz::XXXL),
                Axis::Vertical => Size::new(sz::XXXL, sz::BORDER_STRONG),
            };
            let mut row = ui.layout(flex::row().align(Align::Center).justify(Justify::Center));
            row.child()
                .item(flex::item().fixed(marker.width, marker.height))
                .insert(
                    Rectangle::new()
                        .background(if active { colors::ACCENT } else { colors::BORDER })
                        .radius(BorderRadius::uniform(sz::BORDER)),
                );
        })
        .leading(leading)
        .trailing(trailing)
}

fn draw_grip(ui: Ui<'_>, grip: ResizeGrip) {
    let marker = match grip.edge {
        ResizeEdge::Right => Size::new(sz::XXS, sz::XXXXL),
        ResizeEdge::Bottom => Size::new(sz::XXXXL, sz::XXS),
        ResizeEdge::Corner => Size::uniform(sz::XS),
    };
    let active = grip.hovered || grip.active;
    let color = if active {
        colors::ACCENT
    } else if grip.edge == ResizeEdge::Corner {
        colors::GRIP_CORNER
    } else {
        colors::GRIP
    };
    let mut grip = ui.layout(flex::row().align(Align::Center).justify(Justify::Center));
    grip.child()
        .item(flex::item().fixed(marker.width, marker.height))
        .insert(
            Rectangle::new()
                .background(color)
                .radius(BorderRadius::uniform(marker.width.min(marker.height) / 2.0)),
        );
}

struct Button<'a> {
    label: &'a str,
    selected: bool,
}

impl<'a> Button<'a> {
    fn new(label: &'a str, selected: bool) -> Self {
        Self { label, selected }
    }
}

impl Widget<DesktopContext> for Button<'_> {
    type Response = bool;

    fn build(self, mut ui: Ui<'_>) -> bool {
        let press = ui.interact(interact::press());
        draw_button(ui, self.label, self.selected, press);
        press.clicked
    }
}

fn draw_button(ui: Ui<'_>, label: impl AsRef<str>, selected: bool, press: interact::press::Response) {
    let stops = if selected {
        &colors::BUTTON_SELECTED
    } else if press.hovered {
        &colors::BUTTON_HOVER
    } else {
        &colors::BUTTON
    };
    let border = if selected {
        Border::solid(sz::BORDER, colors::ACCENT)
    } else {
        Border::gradient(sz::BORDER, LinearGradient::new(&colors::RIM).angle(90.0))
    };
    let mut button = ui.layout(flex::row().align(Align::Center).justify(Justify::Center));
    button.insert(
        Rectangle::new()
            .background(LinearGradient::new(stops).angle(if press.active { 270.0 } else { 90.0 }))
            .border(border)
            .radius(BorderRadius::uniform(4.0)),
    );
    button.insert(Pad::new(Sides::xy(sz::SM, sz::XS)));
    button.child().insert(Text::new(label.as_ref()).style(TextStyle {
        size: sz::MD,
        color: if selected { colors::BACKGROUND } else { colors::TEXT },
        ..TextStyle::default()
    }));
}

fn choices<T: Copy + PartialEq>(ui: Ui<'_>, label: &str, selected: &mut T, options: &[(&str, T)]) {
    let mut group = ui.layout(flex::column().gap(sz::XXS));
    group.child().insert(Text::new(label).style(TextStyle {
        size: sz::MD,
        color: colors::TEXT_MUTED,
        ..TextStyle::default()
    }));
    group.child().build(select(
        selected,
        options,
        wrap::horizontal().item_gap(sz::XXS).run_gap(sz::XXS),
        draw_button,
    ));
}

#[derive(Clone, Copy)]
struct Canvas {
    config: CanvasConfig,
    unit: Size<f32>,
}

impl Widget<DesktopContext> for Canvas {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let background = Rectangle::new()
            .background(colors::CANVAS)
            .border(Border::solid(sz::BORDER_STRONG, colors::CANVAS_BORDER))
            .radius(BorderRadius::uniform(sz::XS));
        let ui = ui.id(Id::new("desktop canvas"));
        match self.config.layout {
            CanvasLayout::Flex => {
                let mut canvas = ui
                    .layout(
                        flex::layout(self.config.axis)
                            .gap(self.config.gap(self.config.axis, self.unit))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .clip(BoundsClip);
                canvas.insert(background);
                canvas.insert(Pad::new(self.config.padding(self.unit)));
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    let (width, height) = self.config.item_sizing(index, self.unit);
                    canvas
                        .child()
                        .item(flex::item().width(width).height(height))
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
                        wrap::new(self.config.axis)
                            .item_gap(self.config.gap(self.config.axis, self.unit))
                            .run_gap(self.config.gap(cross, self.unit))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .clip(BoundsClip);
                canvas.insert(background);
                canvas.insert(Pad::new(self.config.padding(self.unit)));
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    let (width, height) = self.config.item_sizing(index, self.unit);
                    canvas
                        .child()
                        .item(wrap::item().width(width).height(height))
                        .build(|ui: Ui<'_>| {
                            canvas_item(ui, index, spec, self.config);
                        });
                }
            }
            CanvasLayout::Grid => {
                let grid = grid::new(5)
                    .column_gap(self.config.gap(Axis::Horizontal, self.unit))
                    .row_gap(self.config.gap(Axis::Vertical, self.unit));
                let mut canvas = ui.layout(grid).clip(BoundsClip);
                canvas.insert(background);
                canvas.insert(Pad::new(self.config.padding(self.unit)));
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    canvas
                        .child()
                        .item(
                            grid::item()
                                .row_span(spec.rows)
                                .column_span(spec.columns)
                                .height(5.0 * self.unit.height * self.config.zoom),
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
    let color = colors::ITEMS[index];
    let stops = [
        GradientStop::new(
            0.0,
            Color::rgb(
                color.red.saturating_add(56),
                color.green.saturating_add(56),
                color.blue.saturating_add(56),
            ),
        ),
        GradientStop::new(
            1.0,
            Color::rgb(
                color.red.saturating_sub(64),
                color.green.saturating_sub(64),
                color.blue.saturating_sub(64),
            ),
        ),
    ];
    let mut item = ui
        .id(Id::new(("canvas item", index)))
        .layout(flex::column().align(Align::Center).justify(Justify::Center));
    item.insert(
        Rectangle::new()
            .background(LinearGradient::new(&stops).angle(45.0))
            .radius(BorderRadius::uniform(sz::XXS)),
    );
    if let Some(motion) = config.transition.motion() {
        item = item.motion(motion);
    }
    item.child().insert(Text::new(spec.label).style(TextStyle {
        size: sz::MD * config.zoom,
        color: Color::WHITE,
        ..TextStyle::default()
    }));
    if let Some(anchor) = spec.badge {
        item.child()
            .parent(Id::new("desktop canvas"))
            .z_index(1)
            .build(|ui: Ui<'_>| {
                let mut badge = ui
                    .place(
                        NodeTarget::Parent,
                        absolute::place()
                            .target_anchor(anchor)
                            .child_anchor(Anchor::Center)
                            .width(absolute::Sizing::fixed(sz::BADGE_WIDTH * config.zoom))
                            .height(absolute::Sizing::fixed(sz::LG * config.zoom)),
                    )
                    .layout(flex::row().align(Align::Center).justify(Justify::Center));
                badge.insert(
                    Rectangle::new()
                        .background(colors::BACKGROUND)
                        .border(Border::solid(sz::BORDER, Color::WHITE))
                        .radius(BorderRadius::uniform(sz::XXS)),
                );
                badge.child().insert(Text::new("ABS").style(TextStyle {
                    size: (sz::XS * config.zoom).max(sz::XS),
                    color: Color::WHITE,
                    ..TextStyle::default()
                }));
            });
    }
}

fn scroll_area(state: &mut ScrollState, content: impl FnOnce(Ui<'_>)) -> impl Widget<DesktopContext> {
    ScrollArea::new(state)
        .clip(BoundsClip)
        .scroll_speed(2.0)
        .inertia_friction(3.0)
        .scrollbar_thickness(sz::XS)
        .minimum_thumb_extent(sz::XXL)
        .build_scrollbar(content, scroll_track, scroll_thumb)
}

fn scroll_track(mut ui: Ui<'_>) {
    ui.insert(
        Rectangle::new()
            .background(colors::SCROLL_TRACK)
            .radius(BorderRadius::uniform(sz::XXS)),
    );
}

fn scroll_thumb(mut ui: Ui<'_>, active: bool) {
    ui.insert(
        Rectangle::new()
            .background(if active { colors::TEXT_DIM } else { colors::BORDER })
            .radius(BorderRadius::uniform(sz::XXS)),
    );
}

fn panel(background: Color) -> Rectangle<'static> {
    Rectangle::new()
        .background(background)
        .border(Border::gradient(
            sz::BORDER,
            LinearGradient::new(&colors::RIM).angle(90.0),
        ))
        .radius(BorderRadius::uniform(2.0))
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
    pub const BADGE_WIDTH: f32 = 36.0;

    pub const CANVAS_INITIAL_SCALE: f32 = 0.8;
    pub const CANVAS_INITIAL_OFFSET: Size<f32> = Size::new(430.0, 150.0);
    pub const CANVAS_INITIAL_MIN: Size<f32> = Size::new(280.0, 220.0);
    pub const CANVAS_MIN: Size<f32> = Size::new(240.0, 180.0);
}

mod graphics;

mod colors {
    use blit_gui::{color::Color, style::GradientStop};

    pub const BACKGROUND: Color = Color::rgb(16, 19, 22);
    pub const SURFACE: Color = Color::rgb(24, 28, 32);
    pub const SURFACE_HIGH: Color = Color::rgb(41, 47, 52);
    pub const TRACK: Color = Color::rgb(19, 22, 25);
    pub const SCROLL_TRACK: Color = Color::rgba(12, 15, 18, 120);
    pub const CANVAS: Color = Color::rgb(30, 35, 39);
    pub const CANVAS_BORDER: Color = Color::rgb(86, 94, 99);
    pub const GRIP: Color = Color::rgb(88, 97, 104);
    pub const GRIP_CORNER: Color = Color::rgb(157, 164, 163);
    pub const BORDER: Color = Color::rgb(62, 70, 76);
    pub const TEXT: Color = Color::rgb(222, 222, 210);
    pub const TEXT_MUTED: Color = Color::rgb(156, 165, 167);
    pub const TEXT_DIM: Color = Color::rgb(116, 128, 135);
    pub const ACCENT: Color = Color::rgb(226, 184, 107);
    pub const ACCENT_DARK: Color = Color::rgb(112, 88, 49);
    pub const BUTTON: [GradientStop; 2] = [
        GradientStop::new(0.0, Color::rgb(39, 45, 50)),
        GradientStop::new(1.0, Color::rgb(26, 30, 34)),
    ];
    pub const INPUT: [GradientStop; 2] = [
        GradientStop::new(0.0, Color::rgb(48, 54, 59)),
        GradientStop::new(1.0, Color::rgb(40, 46, 51)),
    ];
    pub const BUTTON_HOVER: [GradientStop; 2] = [
        GradientStop::new(0.0, Color::rgb(51, 59, 65)),
        GradientStop::new(1.0, Color::rgb(36, 42, 47)),
    ];
    pub const BUTTON_SELECTED: [GradientStop; 2] = [
        GradientStop::new(0.0, Color::rgb(237, 199, 131)),
        GradientStop::new(1.0, Color::rgb(197, 153, 80)),
    ];
    pub const RIM: [GradientStop; 2] = [
        GradientStop::new(0.0, Color::rgb(78, 88, 95)),
        GradientStop::new(1.0, Color::rgb(37, 44, 49)),
    ];
    pub const ITEMS: [Color; 10] = [
        Color::rgb(181, 145, 86),
        Color::rgb(117, 145, 136),
        Color::rgb(180, 107, 77),
        Color::rgb(144, 148, 146),
        Color::rgb(166, 151, 115),
        Color::rgb(109, 132, 147),
        Color::rgb(144, 150, 105),
        Color::rgb(160, 119, 99),
        Color::rgb(133, 130, 145),
        Color::rgb(159, 128, 122),
    ];
}
