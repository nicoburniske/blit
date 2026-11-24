use std::{
    cell::{Cell, RefCell},
    io,
    rc::Rc,
};

use blit::{
    Animation, Axis, Easing, KeyboardEvent, Motion, Scalar, Sides, Size, Spring, Target, Tween, Widget,
    input::{Input, Key},
};
use blit_demo::{CanvasConfig, CanvasLayout, ITEMS, ItemSizing, Transition, board, select};
use blit_tui::{
    BoundsClip, TuiContext, Ui,
    color::Color,
    interact,
    layout::{Align, Justify, Sizing, absolute, absolute::Anchor, flex, grid, wrap},
    text::{HorizontalAlign, Span, Text, TextAttributes, TextOptions, TextOverflow, TextWrap, VerticalAlign},
    widgets::{
        bar_chart::{Bar, BarChart},
        block::{Block, Border, BorderSides, BorderStyle, Title},
        gauge::Gauge,
        performance::{Performance, PerformanceState},
        popover::{Popover, PopoverClose, PopoverState},
        resize::{Resize, ResizeEdge, ResizeGrip, ResizeState},
        scroll::{ScrollState, area::ScrollArea, uniform_list::UniformList},
        shadow::Shadow,
        sparkline::Sparkline,
        split::{Split, SplitState},
        text_input::{TextInput, TextInputState},
        tint::Tint,
    },
};

fn main() -> io::Result<()> {
    let mut demo = Demo::default();
    blit_tui::run(|ui| demo.show(ui))
}

struct Demo {
    page: Page,
    layout: LayoutPage,
    text: TextPage,
    blocks: BlocksPage,
    widgets: WidgetsPage,
    scroll: ScrollPage,
    animation: AnimationPage,
    drag: DragPage,
    settings: PopoverState,
    modal: Modal,
    performance: PerformanceState,
    show_performance: bool,
}

impl Default for Demo {
    fn default() -> Self {
        Self {
            page: Page::default(),
            layout: LayoutPage::default(),
            text: TextPage::default(),
            blocks: BlocksPage::default(),
            widgets: WidgetsPage::default(),
            scroll: ScrollPage::default(),
            animation: AnimationPage::default(),
            drag: DragPage::default(),
            settings: PopoverState::new(),
            modal: Modal::default(),
            performance: PerformanceState::default(),
            show_performance: true,
        }
    }
}

impl Demo {
    fn show(&mut self, mut ui: Ui<'_>) {
        let quit = ui.interact(interact::keyboard(|input| match input {
            KeyboardEvent::Text('q') => Some(()),
            _ => None,
        }));
        if !self.modal.open && quit.is_some() {
            ui.context_mut().should_quit = true;
            return;
        }
        let mut root = ui.layout(flex::column().gap(1)).pad(1);
        root.insert(Block::new().background(colors::BACKGROUND));
        {
            let mut header = root
                .child()
                .item(flex::item().height(1))
                .layout(flex::row().align(Align::Center).gap(1));
            header.insert(Block::new().background(colors::SURFACE));
            header.child().build(|ui: Ui<'_>| {
                let mut logo = ui.layout(flex::row().align(Align::Center));
                logo.insert(Block::new().background(colors::ACCENT));
                logo.child().insert(
                    Text::new(" blit ")
                        .color(colors::SURFACE)
                        .attributes(TextAttributes::BOLD),
                );
            });
            header.child().build(select(
                &mut self.page,
                &[
                    (" layout ", Page::Layout),
                    (" text ", Page::Text),
                    (" blocks ", Page::Blocks),
                    (" widgets ", Page::Widgets),
                    (" scroll ", Page::Scroll),
                    (" animation ", Page::Animation),
                    (" drag ", Page::Drag),
                ],
                flex::row().gap(1),
                draw_button,
            ));
            if header.child().build(&mut self.modal) {
                self.settings.open = false;
            }
            header.child().item(flex::item().grow()).insert(());
            let reset = header.child().build(
                Popover::new(&mut self.settings)
                    .target_anchor(Anchor::BottomRight)
                    .child_anchor(Anchor::TopRight)
                    .open_on_hover(true)
                    .close(PopoverClose::Exit)
                    .trigger(|ui, press, open| {
                        draw_button(ui, " settings ", open, press);
                    })
                    .content(|ui: Ui<'_>| {
                        let mut popup = ui.layout(flex::column().gap(1)).pad(1);
                        popup.insert(
                            Block::new()
                                .background(colors::SURFACE)
                                .border(Border::new(colors::ACCENT).style(BorderStyle::Rounded)),
                        );
                        if popup
                            .child()
                            .build(Button::new(" show performance ", self.show_performance))
                        {
                            self.show_performance = !self.show_performance;
                        }
                        let reset = popup.child().build(Button::new(" reset ", false));
                        if popup.child().build(Button::new(" quit ", false)) {
                            popup.context_mut().should_quit = true;
                        }
                        reset
                    }),
            );
            if reset.unwrap_or(false) {
                *self = Self::default();
            }
        }
        match self.page {
            Page::Layout => root.child().item(flex::item().grow()).build(&mut self.layout),
            Page::Text => root.child().item(flex::item().grow()).build(&mut self.text),
            Page::Blocks => root.child().item(flex::item().grow()).build(&mut self.blocks),
            Page::Widgets => root.child().item(flex::item().grow()).build(&mut self.widgets),
            Page::Scroll => root.child().item(flex::item().grow()).build(&mut self.scroll),
            Page::Animation => root.child().item(flex::item().grow()).build(&mut self.animation),
            Page::Drag => root.child().item(flex::item().grow()).build(&mut self.drag),
        };
        root.context_mut().profiler.set_enabled(self.show_performance);
        if self.show_performance {
            root.child()
                .place(
                    blit::Target::ROOT,
                    absolute::place()
                        .target_anchor(Anchor::BottomRight)
                        .child_anchor(Anchor::BottomRight)
                        .x(-1)
                        .y(-1),
                )
                .layout(())
                .child()
                .build(
                    Performance::new(&mut self.performance)
                        .background(colors::SURFACE_HIGH)
                        .graph_background(colors::TRACK)
                        .color(colors::TEXT)
                        .muted_color(colors::TEXT)
                        .accent(colors::ACCENT),
                );
        }
    }
}

struct AnimationPage {
    spring: Spring,
    right: bool,
    scroll: ScrollState,
    split: SplitState,
}

impl Default for AnimationPage {
    fn default() -> Self {
        Self {
            spring: Spring::new(0.650, 0.5).rest(0.01, 0.01),
            right: false,
            scroll: ScrollState::default(),
            split: SplitState::default(),
        }
    }
}

impl Widget<TuiContext> for &mut AnimationPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let AnimationPage {
            spring,
            right,
            scroll: controls_scroll,
            split: split_state,
        } = self;
        let settings = Cell::new(*spring);
        let mut body = ui.layout(flex::row());
        body.child().item(flex::item().grow()).build(split(
            split_state,
            40,
            |ui: Ui<'_>| {
                let mut sidebar = ui.layout(flex::column()).clip(BoundsClip).pad(1);
                sidebar.insert(panel(colors::SURFACE, " config "));
                sidebar.child().item(flex::item().grow()).build(
                    ScrollArea::new(controls_scroll)
                        .clip(BoundsClip)
                        .minimum_thumb_extent(4)
                        .build_scrollbar(
                            |ui: Ui<'_>| {
                                let mut controls = ui.layout(flex::column().gap(1));
                                controls.child().build(|ui: Ui<'_>| {
                                    choices(
                                        ui,
                                        "bounce",
                                        &mut spring.bounce,
                                        &[("0", 0.0), ("0.25", 0.25), ("0.5", 0.5)],
                                    );
                                });
                                controls.child().build(|ui: Ui<'_>| {
                                    choices(
                                        ui,
                                        "duration",
                                        &mut spring.duration,
                                        &[("350ms", 0.35), ("650ms", 0.65), ("1000ms", 1.0), ("2000ms", 2.0)],
                                    );
                                });
                            },
                            |mut ui: Ui<'_>| {
                                ui.insert(Block::new().background(colors::TRACK));
                            },
                            scroll_thumb,
                        ),
                );
                settings.set(*spring);
            },
            |ui: Ui<'_>| {
                let spring = settings.get();
                let mut preview = ui.layout(flex::column().gap(1)).clip(BoundsClip).pad(1);
                preview.insert(panel(colors::SURFACE, " preview "));
                if preview
                    .child()
                    .item(flex::item().width(Sizing::grow()))
                    .layout(flex::row().justify(Justify::Center))
                    .child()
                    .build(Button::new(if *right { "move left" } else { "move right" }, false))
                {
                    *right = !*right;
                }
                let tween = Animation::Tween(Tween::new(spring.duration, Easing::EASE_IN_OUT));
                let spring = Animation::Spring(spring);
                for (index, (label, animation, restart)) in [
                    ("tween / carry", tween, false),
                    ("tween / restart", tween, true),
                    ("spring / carry", spring, false),
                    ("spring / restart", spring, true),
                ]
                .into_iter()
                .enumerate()
                {
                    let mut row = preview
                        .child()
                        .item(flex::item().width(Sizing::grow()))
                        .layout(flex::row().align(Align::Center).gap(2));
                    row.child()
                        .item(flex::item().width(18))
                        .insert(Text::new(label).color(colors::TEXT));
                    let mut track = row
                        .child()
                        .item(flex::item().width(Sizing::grow()).height(1))
                        .layout(())
                        .clip(BoundsClip);
                    track.insert(Block::new().background(colors::SURFACE_HIGH));
                    let mut range = track
                        .child()
                        .place(
                            Target::PARENT,
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
                        .width(3)
                        .height(1);
                    range.child().place(Target::PARENT, marker).insert(
                        Text::new("│")
                            .color(colors::TEXT)
                            .options(TextOptions::new().horizontal_align(HorizontalAlign::Center)),
                    );
                    range
                        .child_id(index)
                        .place(Target::PARENT, marker)
                        .motion(Motion::new(animation).x().restart(restart))
                        .insert(Block::new().background(if restart { colors::ACCENT_DARK } else { colors::ACCENT }));
                }
            },
        ));
    }
}

/// cards move between columns by drag and drop
struct DragPage {
    columns: [(&'static str, Vec<u32>); 3],
}

impl Default for DragPage {
    fn default() -> Self {
        Self {
            columns: [
                ("todo", vec![1, 2, 3, 4]),
                ("doing", vec![5, 6]),
                ("done", vec![7, 8, 9]),
            ],
        }
    }
}

impl Widget<TuiContext> for &mut DragPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        ui.build(board(
            &mut self.columns,
            3,
            1,
            |column, name| column.insert(panel(colors::CANVAS, name)),
            |mut ui: Ui<'_>, card, dragging| {
                let Some(card) = card else {
                    ui.insert(Block::new().border(Border::new(colors::CANVAS_BORDER).style(BorderStyle::Rounded)));
                    return;
                };
                let border = if dragging { colors::TEXT } else { colors::BORDER };
                let mut ui = ui.layout(flex::row().align(Align::Center)).pad(Sides::xy(2, 0));
                ui.insert(
                    Block::new()
                        .background(colors::ITEMS[card as usize % colors::ITEMS.len()])
                        .border(Border::new(border).style(BorderStyle::Rounded)),
                );
                ui.child().insert(
                    Text::new(format_args!("card {card}"))
                        .color(colors::TEXT)
                        .attributes(TextAttributes::BOLD),
                );
            },
        ));
    }
}

struct Modal {
    open: bool,
    palette: PopoverState,
    accent: usize,
    dim: bool,
}

impl Default for Modal {
    fn default() -> Self {
        Self {
            open: false,
            palette: PopoverState::new(),
            accent: 0,
            dim: true,
        }
    }
}

impl Widget<TuiContext> for &mut Modal {
    type Response = bool;

    fn build(self, ui: Ui<'_>) -> bool {
        if let Input::Key(key) = ui.input()
            && key.key == Key::Escape
            && key.pressed
            && !key.repeat
        {
            if self.palette.open {
                self.palette.open = false;
            } else {
                self.open = false;
            }
        }
        let mut root = ui.layout(flex::column());
        let opened = root.child().build(Button::new(" modal ", self.open));
        if opened {
            self.open = true;
            self.palette.open = false;
        }
        let backdrop_id = "backdrop";
        let backdrop = Target::ROOT.scoped(backdrop_id);
        if self.open && root.at(backdrop).interact(interact::press()).clicked {
            self.open = false;
        }
        if !self.open {
            return opened;
        }
        let palettes = [
            (" ocean ", Color::Rgb(100, 210, 255)),
            (" ember ", Color::Rgb(255, 165, 100)),
            (" orchid ", Color::Rgb(205, 155, 255)),
        ];
        let (_, accent) = palettes[self.accent];
        let panel_id = "panel";
        let mut modal = root
            .child_in(blit::Scope::ROOT, backdrop_id)
            .parent(blit::Target::ROOT)
            .z_index(1)
            .place(
                blit::Target::ROOT,
                absolute::place()
                    .width(absolute::Sizing::full())
                    .height(absolute::Sizing::full()),
            )
            .layout(());
        modal.insert(Tint::new([0, 0, 0], if self.dim { 180 } else { 80 }));
        let mut panel = modal.child_id(panel_id);
        panel.interact(interact::press());
        let mut panel = panel
            .place(
                Target::PARENT,
                absolute::place()
                    .target_anchor(Anchor::Center)
                    .child_anchor(Anchor::Center),
            )
            .layout(flex::column().gap(1))
            .pad(1);
        panel.insert(
            Block::new()
                .background(colors::SURFACE)
                .border(Border::new(accent).style(BorderStyle::Rounded)),
        );
        panel
            .child()
            .insert(Text::new("Appearance").color(accent).attributes(TextAttributes::BOLD));
        panel.child().build(|ui: Ui<'_>| {
            let mut preview = ui.layout(flex::column()).pad(1);
            preview.insert(
                Block::new()
                    .background(Color::Rgb(24, 28, 38))
                    .border(Border::new(accent).style(BorderStyle::Rounded)),
            );
            preview
                .child()
                .insert(Text::new("██  Preview").color(accent).attributes(TextAttributes::BOLD));
        });
        let selected = panel.child().build(
            Popover::new(&mut self.palette)
                .parent(backdrop)
                .trigger(|ui, press, open| {
                    draw_button(ui, " Accent color ▾ ", open, press);
                })
                .content(|ui: Ui<'_>| {
                    let mut menu = ui.layout(flex::column()).pad(1);
                    menu.insert(
                        Block::new()
                            .background(colors::SURFACE)
                            .border(Border::new(accent).style(BorderStyle::Rounded)),
                    );
                    let mut selected = None;
                    for (index, (name, _)) in palettes.into_iter().enumerate() {
                        if menu.child_id(index).build(Button::new(name, self.accent == index)) {
                            selected = Some(index);
                        }
                    }
                    selected
                }),
        );
        if let Some(Some(index)) = selected {
            self.accent = index;
            self.palette.open = false;
        }
        let mut actions = panel.child().layout(flex::row().gap(2));
        if actions.child().build(Button::new(" Dim background ", self.dim)) {
            self.dim = !self.dim;
        }
        if actions.child().build(Button::new(" Done ", false)) {
            self.open = false;
        }

        opened
    }
}

#[derive(Default)]
struct LayoutPage {
    canvas: CanvasConfig,
    resize: ResizeState,
    scroll: ScrollState,
    split: SplitState,
}

impl Widget<TuiContext> for &mut LayoutPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let LayoutPage {
            canvas,
            resize,
            scroll: layout_scroll,
            split: split_state,
        } = self;
        let screen = ui.screen();
        let screen_size = screen.size();
        let preview_config = *canvas;
        let mut body = ui.layout(flex::row());
        body.child().item(flex::item().grow()).build(split(
            split_state,
            40,
            |ui: Ui<'_>| {
                let mut sidebar = ui.layout(flex::column()).pad(1);
                sidebar.insert(panel(colors::SURFACE, " config "));
                sidebar.child().item(flex::item().grow()).build(
                    ScrollArea::new(layout_scroll)
                        .clip(BoundsClip)
                        .minimum_thumb_extent(4)
                        .build_scrollbar(
                            |ui: Ui<'_>| {
                                let mut controls = ui.layout(flex::column().gap(1));
                                controls.child().build(|ui: Ui<'_>| {
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
                                if matches!(canvas.layout, CanvasLayout::Flex | CanvasLayout::Wrap) {
                                    controls.child().build(|ui: Ui<'_>| {
                                        choices(
                                            ui,
                                            "axis",
                                            &mut canvas.axis,
                                            &[(" Horz ", Axis::Horizontal), (" Vert ", Axis::Vertical)],
                                        );
                                    });
                                    controls.child().build(|ui: Ui<'_>| {
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
                                    controls.child().build(|ui: Ui<'_>| {
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
                                    controls.child().build(|ui: Ui<'_>| {
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
                                }
                                controls.child().build(|ui: Ui<'_>| {
                                    choices(
                                        ui,
                                        "zoom",
                                        &mut canvas.zoom,
                                        &[(" 75% ", 0.75), (" 100% ", 1.0), (" 125% ", 1.25)],
                                    );
                                });
                                controls.child().build(|ui: Ui<'_>| {
                                    choices(
                                        ui,
                                        "gap",
                                        &mut canvas.gap_steps,
                                        &[(" 0 ", 0), (" 1 ", 1), (" 2 ", 2), (" 3 ", 3)],
                                    );
                                });
                                controls.child().build(|ui: Ui<'_>| {
                                    choices(
                                        ui,
                                        "padding",
                                        &mut canvas.padding_steps,
                                        &[(" 0 ", 0), (" 1 ", 1), (" 2 ", 2), (" 3 ", 3)],
                                    );
                                });
                                controls.child().build(|ui: Ui<'_>| {
                                    choices(
                                        ui,
                                        "transition",
                                        &mut canvas.transition,
                                        &[
                                            (" Off ", Transition::Off),
                                            (" Spring ", Transition::Spring),
                                            (" Tween ", Transition::Tween),
                                        ],
                                    );
                                });
                            },
                            |mut ui: Ui<'_>| {
                                ui.insert(Block::new().background(colors::TRACK));
                            },
                            scroll_thumb,
                        ),
                );
            },
            |ui: Ui<'_>| {
                let mut preview = ui.layout(flex::column()).pad(1);
                preview.insert(panel(colors::SURFACE, " preview "));
                {
                    let mut viewport = preview
                        .child()
                        .item(flex::item().grow())
                        .layout(flex::column())
                        .clip(BoundsClip)
                        .pad(1);
                    viewport.insert(Block::new().background(colors::TRACK));
                    viewport.child().build(
                        Resize::new(resize)
                            .initial(Size::new(64, 16))
                            .minimum(Size::new(18, 9))
                            .maximum(screen_size)
                            .content(Canvas { config: preview_config })
                            .grip(draw_grip),
                    );
                }
            },
        ));
    }
}

struct TextPage {
    input: String,
    input_state: TextInputState,
    resize: ResizeState,
    attributes: TextAttributes,
    wrap: TextWrap,
    overflow: TextOverflow,
    horizontal: HorizontalAlign,
    vertical: VerticalAlign,
    max_lines: Option<u16>,
    split: SplitState,
}

impl Default for TextPage {
    fn default() -> Self {
        Self {
            input: String::new(),
            input_state: TextInputState::default(),
            resize: ResizeState::default(),
            attributes: TextAttributes::NONE,
            wrap: TextWrap::Word,
            overflow: TextOverflow::Clip,
            horizontal: HorizontalAlign::Left,
            vertical: VerticalAlign::Top,
            max_lines: None,
            split: SplitState::default(),
        }
    }
}

impl Widget<TuiContext> for &mut TextPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let TextPage {
            input,
            input_state,
            resize: text_resize,
            attributes: text_attributes,
            wrap: text_wrap,
            overflow: text_overflow,
            horizontal: text_horizontal,
            vertical: text_vertical,
            max_lines: text_max_lines,
            split: split_state,
        } = self;
        let screen = ui.screen();
        let screen_size = screen.size();
        let preview_attributes = *text_attributes;
        let preview_wrap = *text_wrap;
        let preview_overflow = *text_overflow;
        let preview_horizontal = *text_horizontal;
        let preview_vertical = *text_vertical;
        let preview_max_lines = *text_max_lines;
        let mut body = ui.layout(flex::row());
        body.child().item(flex::item().grow()).build(split(
            split_state,
            40,
            |ui: Ui<'_>| {
                let mut controls = ui.layout(flex::column().gap(1)).pad(1);
                controls.insert(panel(colors::SURFACE, " config "));
                controls.child().build(|ui: Ui<'_>| {
                    let mut group = ui.layout(flex::column());
                    group.child().insert(Text::new("attributes").color(colors::TEXT_MUTED));
                    let mut toggles = group.child().layout(
                        wrap::horizontal()
                            .item_gap(1)
                            .run_gap(1)
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
                        if toggles
                            .child_id(index)
                            .build(Button::new(label, selected))
                        {
                            text_attributes.set(attribute, !selected);
                        }
                    }
                });
                controls.child().build(|ui: Ui<'_>| {
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
                controls.child().build(|ui: Ui<'_>| {
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
                controls.child().build(|ui: Ui<'_>| {
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
                controls.child().build(|ui: Ui<'_>| {
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
                controls.child().build(|ui: Ui<'_>| {
                    choices(
                        ui,
                        "max lines",
                        text_max_lines,
                        &[(" All ", None), (" 3 ", Some(3)), (" 6 ", Some(6))],
                    );
                });
            },
            |ui: Ui<'_>| {
                let mut preview = ui.layout(flex::column().gap(1)).pad(1);
                {
                    let mut input_preview = preview
                        .child()
                        .item(flex::item().width(Sizing::grow()).height(3));
                    input_preview.insert(panel(colors::SURFACE, " input "));
                    input_preview.build(
                        TextInput::new(input_state, input)
                            .placeholder("type here")
                            .padding(Sides::all(1))
                            .color(colors::TEXT)
                            .placeholder_color(colors::TEXT_DIM)
                            .background(colors::TRACK)
                            .selection_background(colors::SELECTED)
                            .cursor_background(colors::ACCENT_DARK),
                    );
                }
                {
                    let mut resizable = preview
                        .child()
                        .item(flex::item().grow())
                        .layout(()).pad(1);
                    resizable.insert(panel(colors::SURFACE, " text "));
                    let mut viewport = resizable
                        .child()
                        .layout(flex::column())
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
                    viewport.child().build(
                        Resize::new(text_resize).initial(Size::new(64, 16))
                                .minimum(Size::new(12, 6))
                                .maximum(screen_size).content(|ui: Ui<'_>| {
                                let mut paragraph = ui
                                    .layout(flex::column())
                                    .clip(BoundsClip).pad(1);
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
                                paragraph.child().item(flex::item().grow()).insert(
                                    Text::rich(&sample)
                                        .color(colors::TEXT)
                                        .attributes(preview_attributes)
                                        .options(options),
                                );
                            }).grip(draw_grip),
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
    split: SplitState,
}

impl Default for BlocksPage {
    fn default() -> Self {
        Self {
            style: BorderStyle::Rounded,
            sides: BorderSides::ALL,
            shadow: true,
            background: true,
            split: SplitState::default(),
        }
    }
}

impl Widget<TuiContext> for &mut BlocksPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let BlocksPage {
            style: block_style,
            sides: block_sides,
            shadow: block_shadow,
            background: block_background,
            split: split_state,
        } = self;
        let preview_style = *block_style;
        let preview_sides = *block_sides;
        let preview_shadow = *block_shadow;
        let preview_background = *block_background;
        let mut body = ui.layout(flex::row());
        body.child().item(flex::item().grow()).build(split(
            split_state,
            40,
            |ui: Ui<'_>| {
                let mut controls = ui.layout(flex::column().gap(1)).pad(1);
                controls.insert(panel(colors::SURFACE, " config "));
                controls.child().build(|ui: Ui<'_>| {
                    choices(
                        ui,
                        "border",
                        block_style,
                        &[
                            (" Single ", BorderStyle::Single),
                            (" Rounded ", BorderStyle::Rounded),
                            (" Double ", BorderStyle::Double),
                            (" Heavy ", BorderStyle::Heavy),
                        ],
                    );
                });
                controls.child().build(|ui: Ui<'_>| {
                    choices(
                        ui,
                        "sides",
                        block_sides,
                        &[
                            (" All ", BorderSides::ALL),
                            (" Horizontal ", BorderSides::TOP | BorderSides::BOTTOM),
                            (" Vertical ", BorderSides::LEFT | BorderSides::RIGHT),
                            (" None ", BorderSides::NONE),
                        ],
                    );
                });
                controls.child().build(|ui: Ui<'_>| {
                    choices(ui, "shadow", block_shadow, &[(" On ", true), (" Off ", false)]);
                });
                controls.child().build(|ui: Ui<'_>| {
                    choices(ui, "background", block_background, &[(" On ", true), (" Off ", false)]);
                });
            },
            |ui: Ui<'_>| {
                let mut preview = ui.layout(flex::column().gap(1)).pad(1);
                preview.insert(panel(colors::SURFACE, " preview "));
                preview.child().item(flex::item().grow()).build(|ui: Ui<'_>| {
                    let mut block = Block::new()
                        .border(
                            Border::new(colors::CANVAS_BORDER)
                                .style(preview_style)
                                .sides(preview_sides),
                        )
                        .title(
                            Title::new(" block ")
                                .color(colors::ACCENT)
                                .attributes(TextAttributes::BOLD),
                        );
                    if preview_background {
                        block = block.background(colors::SURFACE_HIGH);
                    }
                    if preview_shadow {
                        block = block.shadow(Shadow::new(colors::SHADOW));
                    }
                    let mut configured = ui
                        .layout(flex::column().align(Align::Center).justify(Justify::Center))
                        .pad(1);
                    configured.insert(block);
                });
            },
        ));
    }
}

struct WidgetsPage {
    sparkline: Rc<RefCell<Vec<u64>>>,
    bars: Rc<RefCell<Vec<Bar>>>,
}

impl Default for WidgetsPage {
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

impl Widget<TuiContext> for &mut WidgetsPage {
    type Response = ();

    fn build(self, mut ui: Ui<'_>) {
        let phase = ui.animate_loop(Tween::new(3.0, Easing::LINEAR));
        let ratio = (phase * std::f32::consts::TAU).sin() * 0.25 + 0.5;
        for (x, value) in self.sparkline.borrow_mut().iter_mut().enumerate() {
            *value = (((x as f32 * 0.24 + phase * std::f32::consts::TAU).sin() + 1.0) * 50.0) as u64;
        }

        let mut body = ui.layout(flex::column().gap(1)).pad(1);
        body.insert(panel(colors::SURFACE, " widgets "));
        body.child().item(flex::item().width(Sizing::grow()).height(1)).insert(
            Gauge::new(ratio as f64)
                .filled(colors::ACCENT)
                .unfilled(colors::TRACK)
                .label_color(colors::SURFACE),
        );
        body.child().item(flex::item().height(5)).insert(
            Sparkline::new(self.sparkline.clone())
                .maximum(100)
                .color(colors::ACCENT)
                .background(colors::CANVAS),
        );
        body.child().item(flex::item().grow()).insert(
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
    state: ScrollState,
    items: Vec<String>,
}

impl Default for ScrollPage {
    fn default() -> Self {
        Self {
            axis: Axis::default(),
            state: ScrollState::default(),
            items: (1..=100).map(|index| index.to_string()).collect(),
        }
    }
}

impl Widget<TuiContext> for &mut ScrollPage {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let ScrollPage {
            axis: scroll_axis,
            state: scroll,
            items,
        } = self;
        let mut body = ui.layout(flex::column().gap(1)).pad(1);
        body.insert(panel(colors::SURFACE, " scroll "));
        {
            let mut controls = body.child().layout(flex::column());
            controls.child().insert(Text::new("axis").color(colors::TEXT_MUTED));
            if controls.child().build(select(
                scroll_axis,
                &[(" Vertical ", Axis::Vertical), (" Horizontal ", Axis::Horizontal)],
                flex::row().gap(1),
                draw_button,
            )) {
                *scroll = ScrollState::default();
            }
        }
        let axis = *scroll_axis;
        let item_extent = match axis {
            Axis::Horizontal => 12,
            Axis::Vertical => 3,
        };
        body.child().item(flex::item().grow()).build(
            UniformList::new(scroll, items.iter().enumerate())
                .item_extent(item_extent)
                .axis(axis)
                .minimum_thumb_extent(4)
                .clip(BoundsClip)
                .build_scrollbar(
                    move |ui: Ui<'_>, (index, item)| {
                        let (layout, padding) = match axis {
                            Axis::Horizontal => (flex::column().align(Align::Center).justify(Justify::Center), 0),
                            Axis::Vertical => (flex::row().align(Align::Center), 1),
                        };
                        let background = if index.is_multiple_of(2) {
                            colors::SURFACE_HIGH
                        } else {
                            colors::TRACK
                        };
                        let mut tile = ui.layout(layout).pad(padding);
                        tile.insert(
                            Block::new()
                                .background(background)
                                .border(Border::new(colors::CANVAS_BORDER)),
                        );
                        tile.child()
                            .insert(Text::new(item).color(colors::TEXT).attributes(TextAttributes::BOLD));
                    },
                    |mut ui: Ui<'_>| {
                        ui.insert(Block::new().background(colors::TRACK));
                    },
                    scroll_thumb,
                ),
        );
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Page {
    #[default]
    Layout,
    Text,
    Blocks,
    Widgets,
    Scroll,
    Animation,
    Drag,
}

fn draw_grip(ui: Ui<'_>, grip: ResizeGrip) {
    let marker = match grip.edge {
        ResizeEdge::Right => "║",
        ResizeEdge::Bottom => "═══",
        ResizeEdge::Corner => "╝",
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
    grip.child().insert(Text::new(marker).color(color));
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

impl Widget<TuiContext> for Button<'_> {
    type Response = bool;

    fn build(self, mut ui: Ui<'_>) -> bool {
        let press = ui.interact(interact::press());
        draw_button(ui, self.label, self.selected, press);
        press.clicked
    }
}

fn draw_button(ui: Ui<'_>, label: impl AsRef<str>, selected: bool, press: interact::press::Response) {
    let block = if press.active {
        Block::new().background(colors::ACCENT_DARK)
    } else if selected {
        Block::new().background(colors::SELECTED)
    } else if press.hovered {
        Block::new().background(colors::SURFACE_HIGH)
    } else {
        Block::new()
    };
    let mut button = ui
        .layout(flex::row().align(Align::Center).justify(Justify::Center))
        .pad(Sides::x(1));
    button.insert(block);
    button
        .child()
        .insert(Text::new(label.as_ref().trim()).color(colors::TEXT));
}

fn choices<T: Copy + PartialEq>(ui: Ui<'_>, label: &str, selected: &mut T, options: &[(&str, T)]) {
    let mut group = ui.layout(flex::column());
    group.child().insert(Text::new(label).color(colors::TEXT_MUTED));
    group.child().build(select(
        selected,
        options,
        wrap::horizontal().item_gap(1).align(Align::Center),
        draw_button,
    ));
}

#[derive(Clone, Copy)]
struct Canvas {
    config: CanvasConfig,
}

impl Widget<TuiContext> for Canvas {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let unit = Size::uniform(1);
        let background = Block::new()
            .background(colors::CANVAS)
            .border(Border::new(colors::CANVAS_BORDER).style(BorderStyle::Rounded));
        let padding = self.config.padding(unit);
        let gap = |axis| self.config.gap(axis, unit);
        let item_sizing = |index| self.config.item_sizing(index, unit);
        let mut shell = ui.layout(());
        let ui = shell.child_in(blit::Scope::ROOT, "canvas");
        match self.config.layout {
            CanvasLayout::Flex => {
                let mut canvas = ui
                    .layout(
                        flex::layout(self.config.axis)
                            .gap(gap(self.config.axis))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .clip(BoundsClip)
                    .pad(padding);
                canvas.insert(background);
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    let (width, height) = item_sizing(index);
                    canvas
                        .child_id(index)
                        .item(flex::item().width(width).height(height))
                        .build(|ui: Ui<'_>| {
                            canvas_item(ui, index, spec, self.config, unit);
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
                            .item_gap(gap(self.config.axis))
                            .run_gap(gap(cross))
                            .align(self.config.align)
                            .justify(self.config.justify),
                    )
                    .clip(BoundsClip)
                    .pad(padding);
                canvas.insert(background);
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    let (width, height) = item_sizing(index);
                    canvas
                        .child_id(index)
                        .item(wrap::item().width(width).height(height))
                        .build(|ui: Ui<'_>| {
                            canvas_item(ui, index, spec, self.config, unit);
                        });
                }
            }
            CanvasLayout::Grid => {
                let grid = grid::new(5)
                    .column_gap(gap(Axis::Horizontal))
                    .row_gap(gap(Axis::Vertical));
                let mut canvas = ui.layout(grid).clip(BoundsClip).pad(padding);
                canvas.insert(background);
                for (index, spec) in ITEMS.into_iter().enumerate() {
                    canvas
                        .child_id(index)
                        .item(
                            grid::item()
                                .row_span(spec.rows)
                                .column_span(spec.columns)
                                .height(i32::from_f32(3.0 * self.config.zoom)),
                        )
                        .build(|ui: Ui<'_>| {
                            canvas_item(ui, index, spec, self.config, unit);
                        });
                }
            }
        }
    }
}

fn canvas_item(ui: Ui<'_>, index: usize, spec: blit_demo::ItemSpec, config: CanvasConfig, unit: Size<i32>) {
    let mut item = ui.layout(flex::column().align(Align::Center).justify(Justify::Center));
    item.insert(Block::new().background(colors::ITEMS[index]));
    if let Some(motion) = config.transition.motion() {
        item = item.motion(motion);
    }
    item.child().insert(
        Text::new(spec.label)
            .color(colors::TEXT)
            .attributes(TextAttributes::BOLD),
    );
    if let Some(anchor) = spec.badge {
        item.child()
            .parent(Target::ROOT.scoped("canvas"))
            .z_index(1)
            .build(|ui: Ui<'_>| {
                let mut badge = ui
                    .place(
                        Target::PARENT,
                        absolute::place()
                            .target_anchor(anchor)
                            .child_anchor(Anchor::Center)
                            .width(unit.width * 2)
                            .height(unit.height),
                    )
                    .layout(flex::row().align(Align::Center).justify(Justify::Center));
                badge.insert(Block::new().background(colors::ACCENT_DARK));
                badge.child().insert(Text::new("A").color(colors::TEXT));
            });
    }
}

fn split(
    state: &mut SplitState,
    initial_extent: i32,
    leading: impl Widget<TuiContext>,
    trailing: impl Widget<TuiContext>,
) -> impl Widget<TuiContext> {
    Split::new(state)
        .initial_extent(initial_extent)
        .divider(|ui, divider| {
            let active = divider.hovered || divider.active;
            let mut row = ui.layout(flex::row().align(Align::Center).justify(Justify::Center));
            row.child()
                .insert(Text::new("⠿").color(if active { colors::ACCENT } else { colors::TEXT_DIM }));
        })
        .leading(leading)
        .trailing(trailing)
}

fn scroll_thumb(mut ui: Ui<'_>, active: bool) {
    ui.insert(Block::new().background(if active { colors::ACCENT } else { colors::ACCENT_DARK }));
}

fn panel(background: Color, title: &str) -> Block<'_> {
    Block::new()
        .background(background)
        .border(Border::new(colors::BORDER).style(BorderStyle::Rounded))
        .title(Title::new(title).color(colors::ACCENT).attributes(TextAttributes::BOLD))
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
