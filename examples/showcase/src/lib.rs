use std::{collections::VecDeque, fmt::Write, time::Duration};

use blit::{
    Ui,
    animation::{Easing, Transition},
    container::{Absolute, Anchor, LayerId, Sizing, Slot},
    geometry::{LogicalPoint, LogicalSize, Sides},
    image::{
        ImageData, ImageFit, ImageFormat, ImageHandle, ImagePixels, ImageSampling, ImageTiling,
    },
    input::{Input, Key},
    interact::{Sense, WidgetId},
    layout::{
        Align, Axis, Constraints, Flex, Grid, ItemScope, Justify, Layout, LayoutCx,
        LayoutResolution, UnitScope, Wrap,
    },
    style::{Clip, Style},
    text::HorizontalAlign,
    widget::{Image, Rectangle, ScrollArea, ScrollState, Text, TextInput, TextInputState, Widget},
};

pub struct Showcase {
    config: Config,
    canvas: CanvasConfig,
    transition_easing: Easing,
    transition_target: bool,
    carousel: usize,
    resize: ResizeState,
    page: Page,
    controls_scroll: ScrollState,
    scroll: ScrollState,
    name: TextInputState,
    password: TextInputState,
    image: Option<ImageHandle>,
    fps_frame_at: Option<Duration>,
    fps_updated_at: Option<Duration>,
    fps_frames: VecDeque<Duration>,
    fps_label: String,
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Config {
        new(),
        space_xs: f32 = 2.0,
        space_sm: f32 = 4.0,
        space_md: f32 = 8.0,
        space_lg: f32 = 12.0,
        space_xl: f32 = 16.0,
        space_xxl: f32 = 20.0,
        text_sm: f32 = 10.0,
        text_md: f32 = 12.0,
        text_lg: f32 = 16.0,
        text_xl: f32 = 23.0,
        sz_xs: f32 = 4.0,
        sz_sm: f32 = 24.0,
        sz_md: f32 = 32.0,
        sz_lg: f32 = 96.0,
        sz_xl: f32 = 160.0,
        border: f32 = 1.0,
        radius_sm: f32 = 5.0,
        radius_md: f32 = 8.0,
        radius_lg: f32 = 11.0,
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

impl Config {
    pub fn terminal() -> Self {
        Self::default()
            .space_xs(0.5)
            .space_sm(1.0)
            .space_md(1.0)
            .space_lg(1.0)
            .space_xl(1.0)
            .space_xxl(2.0)
            .text_sm(1.0)
            .text_md(1.0)
            .text_lg(1.0)
            .text_xl(1.0)
            .sz_xs(0.5)
            .sz_sm(1.0)
            .sz_md(2.0)
            .sz_lg(4.0)
            .sz_xl(6.0)
            .border(0.5)
            .radius_sm(0.5)
            .radius_md(0.5)
            .radius_lg(0.5)
    }
}

#[derive(Clone, Copy)]
struct CanvasConfig {
    layout: CanvasLayout,
    axis: Axis,
    justify: Justify,
    align: Align,
    sizing: ItemSizing,
    gap: Spacing,
    padding: Spacing,
    transitions: bool,
    transition_duration: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Layout,
    Scrolling,
    Input,
    Images,
    Animation,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemSizing {
    Fixed,
    Fit,
    Grow,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CanvasLayout {
    Flex,
    Wrap,
    Grid,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Spacing {
    Zero,
    Xs,
    Sm,
    Md,
    Lg,
}

impl Spacing {
    fn resolve(self, config: Config) -> f32 {
        match self {
            Self::Zero => 0.0,
            Self::Xs => config.space_xs,
            Self::Sm => config.space_sm,
            Self::Md => config.space_md,
            Self::Lg => config.space_lg,
        }
    }
}

impl Default for CanvasConfig {
    fn default() -> Self {
        Self {
            layout: CanvasLayout::Flex,
            axis: Axis::Horizontal,
            justify: Justify::Start,
            align: Align::Center,
            sizing: ItemSizing::Fixed,
            gap: Spacing::Md,
            padding: Spacing::Md,
            transitions: true,
            transition_duration: 300.0,
        }
    }
}

impl Default for Showcase {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl Showcase {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            canvas: CanvasConfig::default(),
            transition_easing: Easing::EaseInOutQuad,
            transition_target: false,
            carousel: 0,
            resize: ResizeState::default(),
            page: Page::Layout,
            controls_scroll: ScrollState::default(),
            scroll: ScrollState::default(),
            name: TextInputState::default(),
            password: TextInputState::default(),
            image: None,
            fps_frame_at: None,
            fps_updated_at: None,
            fps_frames: VecDeque::new(),
            fps_label: "FPS --".into(),
        }
    }

    pub fn set_page(&mut self, page: Page) {
        self.page = page;
    }

    pub fn render(&mut self, ui: &mut Ui) {
        let scale_factor = match ui.input() {
            Input::Key(key) if key.pressed && key.modifiers.control() => match key.key {
                Key::Character('+') | Key::Character('=') => Some(ui.zoom() + 0.25),
                Key::Character('-') => Some(ui.zoom() - 0.25),
                _ => None,
            },
            _ => None,
        };
        if let Some(scale_factor) = scale_factor {
            ui.set_zoom(scale_factor.clamp(0.5, 4.0));
        }

        let now = ui.time();
        if self.fps_frame_at.replace(now) != Some(now) {
            self.fps_frames.push_back(now);
            while self
                .fps_frames
                .front()
                .is_some_and(|frame| now.saturating_sub(*frame) > Duration::from_secs(1))
            {
                self.fps_frames.pop_front();
            }
            if self
                .fps_updated_at
                .is_none_or(|updated| now.saturating_sub(updated) >= Duration::from_millis(250))
            {
                self.fps_updated_at = Some(now);
                if self.fps_frames.len() > 1
                    && let (Some(first), Some(last)) =
                        (self.fps_frames.front(), self.fps_frames.back())
                {
                    let elapsed = last.saturating_sub(*first);
                    let fps =
                        self.fps_frames.len().saturating_sub(1) as f32 / elapsed.as_secs_f32();
                    self.fps_label.clear();
                    let _ = write!(self.fps_label, "FPS {fps:03.0}");
                }
            }
        }

        ui.clear();
        let config = self.config;
        let screen = ui.screen();
        let compact = screen.width < config.text_md * 60.0;
        let mut root = ui
            .layout(
                Flex::column()
                    .padding(Sides::all(config.space_xxl))
                    .gap(config.space_xl),
            )
            .grow()
            .style(Style::new().background(colors::BACKGROUND))
            .open();

        root.add(|ui: &mut Ui| self.header(ui));
        root.add(|ui: &mut Ui| match self.page {
            Page::Layout => {
                let mut body = ui
                    .layout(
                        Flex::new(if compact {
                            Axis::Vertical
                        } else {
                            Axis::Horizontal
                        })
                        .gap(self.config.space_xl),
                    )
                    .grow()
                    .clip(Clip::Bounds)
                    .open();
                body.add(|ui: &mut Ui| self.controls(ui, compact));
                body.add(|ui: &mut Ui| self.preview(ui));
            }
            Page::Scrolling => self.scrolling(ui),
            Page::Input => self.input_page(ui),
            Page::Images => self.images(ui),
            Page::Animation => self.animation_page(ui),
        });
        root.add(|ui: &mut Ui| self.screen_badge(ui));
    }
    fn header(&mut self, ui: &mut Ui) {
        let config = self.config;
        let mut header = ui
            .layout(
                Wrap::horizontal()
                    .item_gap(config.space_md)
                    .run_gap(config.space_sm)
                    .align(Align::Center)
                    .justify(Justify::SpaceBetween),
            )
            .width(Sizing::grow())
            .height(Sizing::fit().min(config.text_xl + config.space_md * 2.0))
            .style(Style::new().background(colors::BACKGROUND))
            .open();
        header.add(
            Text::new("BLIT / SHOWCASE")
                .color(colors::TEXT)
                .text_size(config.text_xl),
        );
        header.add(|ui: &mut Ui| {
            let mut tabs = ui
                .layout(
                    Wrap::horizontal()
                        .padding(Sides::all(config.space_sm))
                        .item_gap(config.space_xs)
                        .run_gap(config.space_xs),
                )
                .style(
                    Style::new()
                        .background(colors::TRACK)
                        .uniform_radius(config.radius_md),
                )
                .open();
            for (index, (label, page)) in [
                ("Layout", Page::Layout),
                ("Scrolling", Page::Scrolling),
                ("Input", Page::Input),
                ("Images", Page::Images),
                ("Animation", Page::Animation),
            ]
            .into_iter()
            .enumerate()
            {
                let selected = self.page == page;
                if tabs.add(
                    Button::new(WidgetId::new(("page", index)))
                        .label(label)
                        .background(if selected {
                            colors::SURFACE_HIGH
                        } else {
                            colors::TRACK
                        })
                        .clicked_background(colors::ACCENT_DARK)
                        .text_color(if selected {
                            colors::TEXT
                        } else {
                            colors::TEXT_MUTED
                        })
                        .border_width(config.border)
                        .border_color(if selected {
                            colors::ACCENT
                        } else {
                            colors::TRACK
                        })
                        .radius(config.radius_sm)
                        .padding_x(config.space_lg)
                        .padding_y(config.space_md)
                        .text_size(config.text_md)
                        .min_height(config.sz_md),
                ) {
                    self.page = page;
                }
            }
        });
        if header.add(choice("reset playground", "Reset", false, config).padding_x(config.space_lg))
        {
            self.canvas = CanvasConfig::default();
            self.transition_easing = Easing::EaseInOutQuad;
            self.transition_target = false;
            self.carousel = 0;
            self.resize = ResizeState::default();
        }
    }

    fn controls(&mut self, ui: &mut Ui, compact: bool) {
        let config = self.config;
        let mut panel = ui
            .layout(Flex::column())
            .width(if compact {
                Sizing::grow()
            } else {
                Sizing::percent(0.28)
            })
            .height(if compact {
                Sizing::percent(0.45)
            } else {
                Sizing::grow()
            })
            .style(
                Style::new()
                    .background(colors::SURFACE)
                    .solid_border(config.border, colors::BORDER)
                    .uniform_radius(config.radius_lg),
            )
            .clip(Clip::Bounds)
            .open();
        panel.add(|ui: &mut Ui| {
            let mut controls = ScrollArea::vertical(&mut self.controls_scroll)
                .id("layout controls")
                .padding(Sides::all(config.space_lg))
                .gap(config.space_md)
                .begin(ui);
            controls.add(
                Text::new("LAYOUT PARAMETERS")
                    .color(colors::ACCENT)
                    .text_size(config.text_md),
            );
            controls.add(
                Text::new("layout")
                    .color(colors::TEXT_MUTED)
                    .text_size(config.text_sm),
            );
            controls.add(options(
                "layout",
                &mut self.canvas.layout,
                [
                    ("Flex", CanvasLayout::Flex),
                    ("Wrap", CanvasLayout::Wrap),
                    ("Grid", CanvasLayout::Grid),
                ],
                config,
            ));

            controls.add(
                Text::new("item transitions")
                    .color(colors::TEXT_MUTED)
                    .text_size(config.text_sm),
            );
            controls.add(options(
                "item transitions",
                &mut self.canvas.transitions,
                [("On", true), ("Off", false)],
                config,
            ));
            controls.add(
                Text::new("transition time")
                    .color(colors::TEXT_MUTED)
                    .text_size(config.text_sm),
            );
            controls.add(options(
                "transition time",
                &mut self.canvas.transition_duration,
                [
                    ("0ms", 0.0),
                    ("150ms", 150.0),
                    ("300ms", 300.0),
                    ("600ms", 600.0),
                    ("1s", 1000.0),
                ],
                config,
            ));

            controls.add(
                Text::new("axis")
                    .color(colors::TEXT_MUTED)
                    .text_size(config.text_sm),
            );
            controls.add(options(
                "axis",
                &mut self.canvas.axis,
                [
                    ("Horizontal", Axis::Horizontal),
                    ("Vertical", Axis::Vertical),
                ],
                config,
            ));

            controls.add(
                Text::new("justify")
                    .color(colors::TEXT_MUTED)
                    .text_size(config.text_sm),
            );
            controls.add(options(
                "justify start",
                &mut self.canvas.justify,
                [
                    ("Start", Justify::Start),
                    ("Center", Justify::Center),
                    ("End", Justify::End),
                ],
                config,
            ));
            controls.add(options(
                "justify space",
                &mut self.canvas.justify,
                [
                    ("Between", Justify::SpaceBetween),
                    ("Around", Justify::SpaceAround),
                    ("Evenly", Justify::SpaceEvenly),
                ],
                config,
            ));

            controls.add(
                Text::new("align")
                    .color(colors::TEXT_MUTED)
                    .text_size(config.text_sm),
            );
            controls.add(options(
                "align",
                &mut self.canvas.align,
                [
                    ("Start", Align::Start),
                    ("Center", Align::Center),
                    ("End", Align::End),
                    ("Stretch", Align::Stretch),
                ],
                config,
            ));

            controls.add(
                Text::new("item sizing")
                    .color(colors::TEXT_MUTED)
                    .text_size(config.text_sm),
            );
            controls.add(options(
                "sizing",
                &mut self.canvas.sizing,
                [
                    ("Fixed", ItemSizing::Fixed),
                    ("Fit", ItemSizing::Fit),
                    ("Grow", ItemSizing::Grow),
                ],
                config,
            ));

            controls.add(
                Text::new("gap")
                    .color(colors::TEXT_MUTED)
                    .text_size(config.text_sm),
            );
            controls.add(options(
                "gap",
                &mut self.canvas.gap,
                [
                    ("0", Spacing::Zero),
                    ("XS", Spacing::Xs),
                    ("SM", Spacing::Sm),
                    ("MD", Spacing::Md),
                    ("LG", Spacing::Lg),
                ],
                config,
            ));

            controls.add(
                Text::new("padding")
                    .color(colors::TEXT_MUTED)
                    .text_size(config.text_sm),
            );
            controls.add(options(
                "padding",
                &mut self.canvas.padding,
                [
                    ("0", Spacing::Zero),
                    ("XS", Spacing::Xs),
                    ("SM", Spacing::Sm),
                    ("MD", Spacing::Md),
                    ("LG", Spacing::Lg),
                ],
                config,
            ));
        });
    }

    fn preview(&mut self, ui: &mut Ui) {
        let config = self.config;
        let mut preview = ui
            .layout(
                Flex::column()
                    .padding(Sides::all(config.space_lg))
                    .gap(config.space_md),
            )
            .grow()
            .style(
                Style::new()
                    .background(colors::SURFACE)
                    .solid_border(config.border, colors::BORDER)
                    .uniform_radius(config.radius_lg),
            )
            .open();
        preview.add(
            Text::new("DRAG THE RIGHT EDGE, BOTTOM EDGE, OR CORNER")
                .color(colors::TEXT_DIM)
                .text_size(config.text_sm),
        );

        preview.add(|ui: &mut Ui| {
            let screen = ui.screen();
            let compact = screen.width < config.text_md * 60.0;
            let mut viewport = ui
                .layout(Flex::column().padding(Sides::all(config.space_md)))
                .grow()
                .style(
                    Style::new()
                        .background(colors::TRACK)
                        .uniform_radius(config.radius_md),
                )
                .clip(Clip::Bounds)
                .open();
            viewport.add(
                Resizable::new(&mut self.resize, config)
                    .id(WidgetId::new("playground canvas"))
                    .width(Sizing::percent(0.85))
                    .height(Sizing::percent(0.8))
                    .min_size(LogicalSize {
                        width: config.sz_xl,
                        height: config.sz_lg,
                    })
                    .max_size(LogicalSize {
                        width: screen.width * if compact { 0.8 } else { 0.7 },
                        height: screen.height * if compact { 0.2 } else { 0.55 },
                    })
                    .content(canvas(self.canvas, config)),
            );
        });
        self.carousel = preview.add(carousel(self.carousel, config));
        preview.add(|ui: &mut Ui| {
            let mut transitions = ui
                .layout(
                    Flex::column()
                        .padding(Sides::all(config.space_md))
                        .gap(config.space_md),
                )
                .width(Sizing::grow())
                .height(Sizing::percent(0.25))
                .style(
                    Style::new()
                        .background(colors::TRACK)
                        .solid_border(config.border, colors::BORDER)
                        .uniform_radius(config.radius_md),
                )
                .open();
            transitions.add(|ui: &mut Ui| self.transition_controls(ui));
            transitions.add(|ui: &mut Ui| self.transition_tracks(ui));
        });
    }

    fn transition_controls(&mut self, ui: &mut Ui) {
        let config = self.config;
        let mut header = ui
            .layout(Flex::row().align(Align::Center).gap(config.space_sm))
            .width(Sizing::grow())
            .open();
        header.add(
            Text::new("TRANSITIONS")
                .color(colors::ACCENT)
                .text_size(config.text_sm),
        );
        header.add(|ui: &mut Ui| {
            let mut choices = ui
                .layout(Flex::row().justify(Justify::End).gap(config.space_sm))
                .grow()
                .open();
            for (index, (label, easing)) in [
                ("Linear", Easing::Linear),
                ("In", Easing::EaseInQuad),
                ("Out", Easing::EaseOutQuad),
                ("In-out", Easing::EaseInOutQuad),
            ]
            .into_iter()
            .enumerate()
            {
                if choices.add(
                    choice(
                        ("transition easing", index),
                        label,
                        self.transition_easing == easing,
                        config,
                    )
                    .padding_y(config.space_sm),
                ) {
                    self.transition_easing = easing;
                }
            }
        });
        if header.add(
            choice(
                "reverse transitions",
                "Reverse",
                self.transition_target,
                config,
            )
            .padding_y(config.space_sm),
        ) {
            self.transition_target = !self.transition_target;
        }
    }

    fn transition_tracks(&self, ui: &mut Ui) {
        let config = self.config;
        let easing = self.transition_easing;
        let mut tracks = ui.layout(Flex::row().gap(config.space_md)).grow().open();
        tracks.add(|ui: &mut Ui| {
            let mut track = ui
                .layout(Flex::row().padding(Sides::all(config.space_sm)))
                .width(Sizing::percent(0.5))
                .height(Sizing::grow())
                .style(
                    Style::new()
                        .background(colors::SURFACE)
                        .uniform_radius(config.radius_sm),
                )
                .clip(Clip::Bounds)
                .open();
            track.add(|ui: &mut Ui| {
                let mut specimen = ui
                    .layout(Flex::row().align(Align::Center).justify(Justify::Center))
                    .width(Sizing::percent(0.2))
                    .height(Sizing::percent(0.4))
                    .id(WidgetId::new("position transition specimen"))
                    .absolute(if self.transition_target {
                        Absolute::attach(Anchor::BottomRight, Anchor::BottomRight)
                    } else {
                        Absolute::attach(Anchor::TopLeft, Anchor::TopLeft)
                    })
                    .transition(
                        Transition::new(Duration::from_millis(700))
                            .easing(easing)
                            .position(),
                    )
                    .style(
                        Style::new()
                            .background(colors::SURFACE_BLUE)
                            .uniform_radius(config.radius_sm),
                    )
                    .open();
                specimen.add(
                    Text::new("X / Y")
                        .color(colors::WHITE)
                        .text_size(config.text_sm),
                );
            });
        });
        tracks.add(|ui: &mut Ui| {
            let mut track = ui
                .layout(Flex::row().padding(Sides::all(config.space_sm)))
                .width(Sizing::percent(0.5))
                .height(Sizing::grow())
                .style(
                    Style::new()
                        .background(colors::SURFACE)
                        .uniform_radius(config.radius_sm),
                )
                .clip(Clip::Bounds)
                .open();
            let (width, height) = if self.transition_target {
                (Sizing::percent(0.45), Sizing::percent(0.7))
            } else {
                (Sizing::percent(0.2), Sizing::percent(0.35))
            };
            track.add(|ui: &mut Ui| {
                let mut specimen = ui
                    .layout(Flex::row().align(Align::Center).justify(Justify::Center))
                    .width(width)
                    .height(height)
                    .id(WidgetId::new("size transition specimen"))
                    .absolute(Absolute::attach(Anchor::Center, Anchor::Center))
                    .transition(
                        Transition::new(Duration::from_millis(700))
                            .easing(easing)
                            .size(),
                    )
                    .style(
                        Style::new()
                            .background(colors::SURFACE_PURPLE)
                            .uniform_radius(config.radius_sm),
                    )
                    .open();
                specimen.add(
                    Text::new("W / H")
                        .color(colors::WHITE)
                        .text_size(config.text_sm),
                );
            });
        });
    }
    fn scrolling(&mut self, ui: &mut Ui) {
        let config = self.config;
        let mut page = ui
            .layout(
                Flex::column()
                    .padding(Sides::all(config.space_xl))
                    .gap(config.space_md),
            )
            .grow()
            .style(
                Style::new()
                    .background(colors::SURFACE)
                    .solid_border(config.border, colors::BORDER)
                    .uniform_radius(config.radius_lg),
            )
            .open();
        page.add(
            Text::new("SCROLLING")
                .color(colors::ACCENT)
                .text_size(config.text_md),
        );
        page.add(
            Text::new("Use the wheel or drag the list. The viewport clips overflowing content and retains its offset.")
                .color(colors::TEXT_MUTED)
                .text_size(config.text_md),
        );
        page.add(|ui: &mut Ui| {
            let mut scroll = ScrollArea::vertical(&mut self.scroll)
                .id("showcase scroll")
                .padding(Sides::all(config.space_md))
                .gap(config.space_md)
                .begin(ui);
            for index in 0..24 {
                scroll.add(|ui: &mut Ui| {
                    let mut row = ui
                        .layout(
                            Flex::row()
                                .padding(Sides::all(config.space_lg))
                                .align(Align::Center)
                                .gap(config.space_lg),
                        )
                        .width(Sizing::grow())
                        .height(Sizing::fit().min(config.text_lg + config.space_lg * 2.0))
                        .style(
                            Style::new()
                                .background(if index % 2 == 0 {
                                    colors::SURFACE_HIGH
                                } else {
                                    colors::CANVAS
                                })
                                .uniform_radius(config.radius_sm),
                        )
                        .open();
                    row.add(
                        Text::new("ITEM")
                            .color(colors::WHITE)
                            .text_size(config.text_lg),
                    );
                    row.add(
                        Text::new("scrollable content")
                            .color(colors::WHITE)
                            .text_size(config.text_md),
                    );
                });
            }
        });
    }

    fn input_page(&mut self, ui: &mut Ui) {
        let config = self.config;
        let mut page = ui
            .layout(
                Flex::column()
                    .padding(Sides::all(config.space_xl))
                    .gap(config.space_lg),
            )
            .grow()
            .style(
                Style::new()
                    .background(colors::SURFACE)
                    .solid_border(config.border, colors::BORDER)
                    .uniform_radius(config.radius_lg),
            )
            .open();
        page.add(
            Text::new("TEXT INPUT / FOCUS")
                .color(colors::ACCENT)
                .text_size(config.text_md),
        );
        page.add(
            Text::new("Click a field, type, use arrow keys, shift to select, and enter to accept.")
                .color(colors::TEXT_MUTED)
                .text_size(config.text_md),
        );
        let name_border = if self.name.focused {
            colors::ACCENT
        } else {
            colors::BORDER
        };
        page.add(
            TextInput::new(&mut self.name)
                .slot(Slot::new().width(Sizing::percent(0.5)))
                .padding(Sides::all(config.space_md))
                .style(
                    Style::new()
                        .background(colors::TRACK)
                        .solid_border(config.border, name_border)
                        .uniform_radius(config.radius_sm),
                )
                .text_color(colors::TEXT)
                .cursor_color(colors::ACCENT)
                .selection_background(colors::ACCENT_DARK)
                .text_size(config.text_lg),
        );
        let password_border = if self.password.focused {
            colors::ACCENT
        } else {
            colors::BORDER
        };
        page.add(
            TextInput::new(&mut self.password)
                .slot(Slot::new().width(Sizing::percent(0.5)))
                .padding(Sides::all(config.space_md))
                .style(
                    Style::new()
                        .background(colors::TRACK)
                        .solid_border(config.border, password_border)
                        .uniform_radius(config.radius_sm),
                )
                .mask('●')
                .text_color(colors::TEXT)
                .cursor_color(colors::ACCENT)
                .selection_background(colors::ACCENT_DARK)
                .text_size(config.text_lg),
        );
    }

    fn images(&mut self, ui: &mut Ui) {
        let config = self.config;
        if self.image.is_none() {
            let mut pixels = Vec::with_capacity(64 * 64 * 4);
            for y in 0..64 {
                for x in 0..64 {
                    pixels.extend_from_slice(&[35 + x * 2, 80 + y * 2, 180 + (x + y) / 3, 255]);
                }
            }
            self.image = Some(ui.create_image(ImageData::new(
                ImagePixels::Owned(pixels.into_boxed_slice()),
                ImageFormat::Rgba8,
                64,
                64,
            )));
        }
        let image = self.image.as_ref().unwrap();
        let mut page = ui
            .layout(
                Flex::column()
                    .padding(Sides::all(config.space_xl))
                    .gap(config.space_lg),
            )
            .grow()
            .style(
                Style::new()
                    .background(colors::SURFACE)
                    .solid_border(config.border, colors::BORDER)
                    .uniform_radius(config.radius_lg),
            )
            .open();
        page.add(
            Text::new("IMAGE RENDERING")
                .color(colors::ACCENT)
                .text_size(config.text_md),
        );
        page.add(
            Text::new(
                "The same RGBA image rendered with nearest, bilinear, cover, and repeat settings.",
            )
            .color(colors::TEXT_MUTED)
            .text_size(config.text_md),
        );
        page.add(|ui: &mut Ui| {
            let mut samples = ui.layout(Flex::row().gap(config.space_lg)).grow().open();
            for (label, fit, sampling, tiling) in [
                (
                    "NEAREST",
                    ImageFit::Fill,
                    ImageSampling::Nearest,
                    ImageTiling::None,
                ),
                (
                    "BILINEAR",
                    ImageFit::Fill,
                    ImageSampling::Bilinear,
                    ImageTiling::None,
                ),
                (
                    "COVER",
                    ImageFit::Cover,
                    ImageSampling::Nearest,
                    ImageTiling::None,
                ),
                (
                    "REPEAT",
                    ImageFit::Fill,
                    ImageSampling::Nearest,
                    ImageTiling::Repeat,
                ),
            ] {
                samples.add(|ui: &mut Ui| {
                    let mut sample = ui
                        .layout(Flex::column().gap(config.space_sm))
                        .width(Sizing::percent(0.25))
                        .open();
                    sample.add(
                        Text::new(label)
                            .color(colors::TEXT_DIM)
                            .text_size(config.text_sm),
                    );
                    sample.add(
                        Image::new(image)
                            .slot(Slot::new().width(Sizing::grow()).height(Sizing::grow()))
                            .fit(fit)
                            .sampling(sampling)
                            .horizontal_tiling(tiling)
                            .vertical_tiling(tiling),
                    );
                });
            }
        });
    }

    fn animation_page(&mut self, ui: &mut Ui) {
        let config = self.config;
        let loop_value = ui.animate_loop(
            WidgetId::new("showcase animation loop"),
            Duration::from_millis(3600),
            Easing::Linear,
        );
        let loop_value = Easing::EaseInOutQuad.apply(if loop_value < 0.5 {
            loop_value * 2.0
        } else {
            2.0 - loop_value * 2.0
        });
        let pulse = ui.animate_loop(
            WidgetId::new("showcase pulse loop"),
            Duration::from_millis(2400),
            Easing::Linear,
        );
        let pulse = Easing::EaseInOutQuad.apply(if pulse < 0.5 {
            pulse * 2.0
        } else {
            2.0 - pulse * 2.0
        });
        let mut page = ui
            .layout(
                Flex::column()
                    .padding(Sides::all(config.space_xl))
                    .gap(config.space_lg),
            )
            .grow()
            .style(
                Style::new()
                    .background(colors::SURFACE)
                    .solid_border(config.border, colors::BORDER)
                    .uniform_radius(config.radius_lg),
            )
            .open();
        page.add(
            Text::new("KEYED LOOPING ANIMATIONS")
                .color(colors::ACCENT)
                .text_size(config.text_md),
        );
        page.add(
            Text::new("These values come from Ui::animate_loop, rather than layout transitions.")
                .color(colors::TEXT_MUTED)
                .text_size(config.text_md),
        );
        page.add(|ui: &mut Ui| {
            let ball_size = config.space_xxl * 2.5;
            let mut track = ui
                .layout(Flex::row())
                .width(Sizing::grow())
                .height(Sizing::percent(0.3))
                .style(
                    Style::new()
                        .background(colors::TRACK)
                        .uniform_radius(config.radius_md),
                )
                .open();
            track.add(|ui: &mut Ui| {
                let travel = ui.screen().width * 0.45;
                ui.layout(Flex::row())
                    .fixed(ball_size, ball_size)
                    .absolute(
                        Absolute::attach(Anchor::Left, Anchor::Left)
                            .offset(loop_value * travel, 0.0),
                    )
                    .style(
                        Style::new()
                            .background(colors::SURFACE_GREEN)
                            .uniform_radius(config.radius_lg),
                    )
                    .open();
            });
        });
        page.add(|ui: &mut Ui| {
            let size = config.space_xxl * (1.5 + pulse * 3.5);
            let mut pulse_box = ui
                .layout(Flex::row().align(Align::Center).justify(Justify::Center))
                .width(Sizing::grow())
                .height(Sizing::percent(0.45))
                .style(
                    Style::new()
                        .background(colors::TRACK)
                        .uniform_radius(config.radius_md),
                )
                .open();
            pulse_box.add(
                Rectangle::new().slot(Slot::new().fixed(size, size)).style(
                    Style::new()
                        .background(colors::SURFACE_PURPLE)
                        .uniform_radius(size / 2.0),
                ),
            );
        });
    }

    fn screen_badge(&self, ui: &mut Ui) {
        let config = self.config;
        let mut screen_badge = ui
            .layout(
                Flex::row()
                    .padding(Sides {
                        top: config.space_sm,
                        right: config.space_md,
                        bottom: config.space_sm,
                        left: config.space_md,
                    })
                    .gap(config.space_sm)
                    .align(Align::Center),
            )
            .style(
                Style::new()
                    .background(colors::SURFACE_HIGH)
                    .solid_border(config.border, colors::ACCENT)
                    .uniform_radius(config.radius_sm),
            )
            .z_index(20)
            .absolute(
                Absolute::screen(0.0, 0.0)
                    .anchors(Anchor::BottomRight, Anchor::BottomRight)
                    .offset(-config.space_xl, -config.space_xl),
            )
            .open();
        screen_badge.add(
            Rectangle::new()
                .slot(Slot::new().fixed(config.space_sm, config.space_sm))
                .style(
                    Style::new()
                        .background(colors::ACCENT)
                        .uniform_radius(config.radius_sm),
                ),
        );
        screen_badge.add(
            Text::new(&self.fps_label)
                .color(colors::TEXT)
                .text_size(config.text_sm),
        );
        screen_badge.add(
            Text::new("SCREEN ABSOLUTE")
                .color(colors::TEXT_DIM)
                .text_size(config.text_sm),
        );
    }
}

#[derive(Clone, Copy)]
struct CarouselLayout {
    active: usize,
}

impl Layout for CarouselLayout {
    type Item = usize;
    type Scope<'a> = ItemScope<'a, Self>;

    fn layout(&self, cx: &mut LayoutCx<'_, Self::Item>, constraints: Constraints) -> LogicalSize {
        let count = cx.children().count();
        if count == 0 {
            return constraints.constrain(LogicalSize::default());
        }
        let active = self.active % count;
        let mut natural = LogicalSize::default();
        for node in cx.children() {
            let child = cx.layout_child(node, Constraints::loose(constraints.max));
            natural.width = natural.width.max(cx.sizing(node, Axis::Horizontal).resolve(
                child.width,
                constraints.max.width,
                true,
            ));
            natural.height = natural.height.max(cx.sizing(node, Axis::Vertical).resolve(
                child.height,
                constraints.max.height,
                true,
            ));
        }
        let card_size = natural;
        let spacing = card_size.width * 0.65;
        natural.width += spacing * 2.0;
        let size = constraints.constrain(natural);
        for node in cx.children() {
            let index = cx.item(node) % count;
            let forward = if index >= active {
                index - active
            } else {
                count - (active - index)
            };
            let backward = count - forward;
            let child = LogicalSize {
                width: card_size.width.min(size.width),
                height: card_size.height.min(size.height),
            };
            cx.constrain_child(node, Constraints::tight(child));
            let distance = forward.min(backward);
            cx.set_z_index(node, -i16::try_from(distance).unwrap_or(i16::MAX));
            let offset = if forward <= backward {
                forward as f32
            } else {
                -(backward as f32)
            } * spacing;
            cx.set_position(
                node,
                LogicalPoint {
                    x: (size.width - child.width) / 2.0 + offset,
                    y: (size.height - child.height) / 2.0,
                },
            );
        }
        size
    }
}

fn carousel(mut active: usize, config: Config) -> impl Widget<Output = usize> {
    move |ui: &mut Ui| {
        const CARDS: [(&str, &str); 5] = [
            ("01", "LAYOUT"),
            ("02", "OVERLAP"),
            ("03", "CLIPPING"),
            ("04", "PAINT ORDER"),
            ("05", "CAROUSEL"),
        ];

        active %= CARDS.len();
        let mut section = ui
            .layout(Flex::column().gap(config.space_sm))
            .width(Sizing::grow())
            .height(Sizing::percent(0.25))
            .open();
        section.add(|ui: &mut Ui| {
            let mut controls = ui
                .layout(Flex::row().align(Align::Center).gap(config.space_sm))
                .width(Sizing::grow())
                .open();
            controls.add(
                Text::new("CAROUSEL / CUSTOM OVERLAP LAYOUT")
                    .color(colors::ACCENT)
                    .text_size(config.text_sm),
            );
            controls.add(Rectangle::new().slot(Slot::new().width(Sizing::grow())));
            if controls.add(choice("carousel previous", "PREV", false, config)) {
                active = (active + CARDS.len() - 1) % CARDS.len();
            }
            if controls.add(choice("carousel next", "NEXT", false, config)) {
                active = (active + 1) % CARDS.len();
            }
        });
        section.add(|ui: &mut Ui| {
            let mut cards = ui
                .layout(CarouselLayout { active })
                .grow()
                .style(
                    Style::new()
                        .background(colors::TRACK)
                        .solid_border(config.border, colors::BORDER)
                        .uniform_radius(config.radius_md),
                )
                .clip(Clip::Bounds)
                .open();
            for (index, &(number, label)) in CARDS.iter().enumerate() {
                cards.add(index, |ui: &mut Ui| {
                    let id = WidgetId::new(("carousel card", index));
                    if ui.interact(id, Sense::CLICK).clicked {
                        active = index;
                    }
                    let mut card = ui
                        .layout(
                            Flex::column()
                                .padding(Sides::all(config.space_lg))
                                .justify(Justify::SpaceBetween),
                        )
                        .width(Sizing::fit().min(config.sz_xl))
                        .height(Sizing::fit().min(config.sz_lg))
                        .id(id)
                        .transition(
                            Transition::new(Duration::from_millis(350))
                                .easing(Easing::EaseOutQuad)
                                .position(),
                        )
                        .style(
                            Style::new()
                                .background(colors::ITEMS[index])
                                .solid_border(
                                    if index == active {
                                        config.border * 2.0
                                    } else {
                                        config.border
                                    },
                                    if index == active {
                                        colors::WHITE
                                    } else {
                                        colors::BORDER
                                    },
                                )
                                .uniform_radius(config.radius_md),
                        )
                        .open();
                    card.add(
                        Text::new(number)
                            .color(colors::WHITE)
                            .text_size(config.text_xl),
                    );
                    card.add(
                        Text::new(label)
                            .color(colors::WHITE)
                            .text_size(config.text_sm),
                    );
                });
            }
        });
        active
    }
}

#[derive(Default)]
struct ResizeState {
    width: Option<f32>,
    height: Option<f32>,
}

blit::builder! {
    struct Resizable<'a> {
        new(state: &'a mut ResizeState, config: Config),
        id: WidgetId = WidgetId::new("resizable"),
        width: Sizing = Sizing::grow(),
        height: Sizing = Sizing::grow(),
        min_size: LogicalSize = LogicalSize::default(),
        max_size: LogicalSize = LogicalSize {
            width: f32::INFINITY,
            height: f32::INFINITY,
        },
    }
}

struct ResizableWidget<'a, W> {
    resizable: Resizable<'a>,
    content: W,
}

impl<'a> Resizable<'a> {
    fn content<W>(self, content: W) -> ResizableWidget<'a, W> {
        ResizableWidget {
            resizable: self,
            content,
        }
    }
}

impl<W: Widget> Widget for ResizableWidget<'_, W> {
    type Output = ();

    fn render(self, ui: &mut Ui) {
        let config = self.resizable;
        let right_id = config.id.child("right grip");
        let bottom_id = config.id.child("bottom grip");
        let corner_id = config.id.child("corner grip");
        let right = ui.interact(right_id, Sense::DRAG);
        let bottom = ui.interact(bottom_id, Sense::DRAG);
        let corner = ui.interact(corner_id, Sense::DRAG);
        let dragging = right.dragging || bottom.dragging || corner.dragging;
        let delta = LogicalPoint {
            x: right.drag_delta.x + corner.drag_delta.x,
            y: bottom.drag_delta.y + corner.drag_delta.y,
        };
        let max_size = LogicalSize {
            width: config.max_size.width.max(config.min_size.width),
            height: config.max_size.height.max(config.min_size.height),
        };
        if delta != LogicalPoint::default() {
            let size = ui
                .geometry(config.id)
                .map_or(config.min_size, |area| LogicalSize {
                    width: area.width,
                    height: area.height,
                });
            if delta.x != 0.0 {
                config.state.width = Some(config.state.width.unwrap_or(size.width) + delta.x);
            }
            if delta.y != 0.0 {
                config.state.height = Some(config.state.height.unwrap_or(size.height) + delta.y);
            }
        }
        if let Some(width) = &mut config.state.width {
            *width = width.clamp(config.min_size.width, max_size.width);
        }
        if let Some(height) = &mut config.state.height {
            *height = height.clamp(config.min_size.height, max_size.height);
        }

        let mut shell = ui
            .layout(Flex::column())
            .id(config.id)
            .width(config.state.width.map_or(config.width, Sizing::fixed))
            .height(config.state.height.map_or(config.height, Sizing::fixed))
            .open();
        shell.add(self.content);
        shell.add(ResizeGrip {
            id: right_id,
            highlighted: right.dragging || right.hovered && !dragging,
            edge: ResizeEdge::Right,
            config: config.config,
        });
        shell.add(ResizeGrip {
            id: bottom_id,
            highlighted: bottom.dragging || bottom.hovered && !dragging,
            edge: ResizeEdge::Bottom,
            config: config.config,
        });
        shell.add(ResizeGrip {
            id: corner_id,
            highlighted: corner.dragging || corner.hovered && !dragging,
            edge: ResizeEdge::Corner,
            config: config.config,
        });
    }
}

struct ResizeGrip {
    id: WidgetId,
    highlighted: bool,
    edge: ResizeEdge,
    config: Config,
}

#[derive(Clone, Copy)]
enum ResizeEdge {
    Right,
    Bottom,
    Corner,
}

impl Widget for ResizeGrip {
    type Output = ();

    fn render(self, ui: &mut Ui) {
        let config = self.config;
        let (border_center, corner_center) = match ui.layout_resolution() {
            LayoutResolution::Continuous => {
                let corner = config.radius_md
                    - (config.radius_md - config.border).max(0.0) / std::f32::consts::SQRT_2;
                (
                    LogicalSize {
                        width: config.border,
                        height: config.border,
                    },
                    LogicalSize {
                        width: corner,
                        height: corner,
                    },
                )
            }
            LayoutResolution::Discrete { step } => {
                let center = LogicalSize {
                    width: step.width / 2.0,
                    height: step.height / 2.0,
                };
                (center, center)
            }
        };
        let (layout, width, height, absolute, z_index, marker_width, marker_height, radius) =
            match self.edge {
                ResizeEdge::Right => (
                    Flex::column().align(Align::Center).justify(Justify::Center),
                    Sizing::fixed(config.space_lg),
                    Sizing::percent(1.0),
                    Absolute::attach(Anchor::Right, Anchor::Center)
                        .offset(-border_center.width, 0.0),
                    1,
                    config.sz_xs,
                    config.sz_md,
                    config.radius_sm,
                ),
                ResizeEdge::Bottom => (
                    Flex::row().align(Align::Center).justify(Justify::Center),
                    Sizing::percent(1.0),
                    Sizing::fixed(config.space_lg),
                    Absolute::attach(Anchor::Bottom, Anchor::Center)
                        .offset(0.0, -border_center.height),
                    1,
                    config.sz_md,
                    config.sz_xs,
                    config.radius_sm,
                ),
                ResizeEdge::Corner => (
                    Flex::row().align(Align::Center).justify(Justify::Center),
                    Sizing::fixed(config.space_lg),
                    Sizing::fixed(config.space_lg),
                    Absolute::attach(Anchor::BottomRight, Anchor::Center)
                        .offset(-corner_center.width, -corner_center.height),
                    2,
                    config.sz_xs,
                    config.sz_xs,
                    config.radius_sm,
                ),
            };
        let mut grip = ui
            .layout(layout)
            .width(width)
            .height(height)
            .id(self.id)
            .hit(Sides::all(config.space_md))
            .z_index(z_index)
            .absolute(absolute)
            .open();
        grip.add(
            Rectangle::new()
                .slot(Slot::new().fixed(marker_width, marker_height))
                .style(
                    Style::new()
                        .background(if self.highlighted {
                            colors::ACCENT
                        } else {
                            colors::GRIP
                        })
                        .uniform_radius(radius),
                ),
        );
    }
}

fn canvas(config: CanvasConfig, theme: Config) -> impl Widget<Output = ()> {
    move |ui: &mut Ui| {
        let gap = config.gap.resolve(theme);
        let padding = config.padding.resolve(theme);
        let style = Style::new()
            .background(colors::CANVAS)
            .solid_border(theme.border * 2.0, colors::CANVAS_BORDER)
            .uniform_radius(theme.radius_md);
        match config.layout {
            CanvasLayout::Flex => {
                let mut layout = ui
                    .layout(
                        Flex::new(config.axis)
                            .padding(Sides::all(padding))
                            .gap(gap)
                            .align(config.align)
                            .justify(config.justify),
                    )
                    .grow()
                    .style(style)
                    .clip(Clip::Bounds)
                    .open();
                canvas_items(&mut layout, config, theme);
            }
            CanvasLayout::Wrap => {
                let mut layout = ui
                    .layout(
                        Wrap::new(config.axis)
                            .padding(Sides::all(padding))
                            .gap(gap)
                            .align(config.align)
                            .justify(config.justify),
                    )
                    .grow()
                    .style(style)
                    .clip(Clip::Bounds)
                    .open();
                canvas_items(&mut layout, config, theme);
            }
            CanvasLayout::Grid => {
                let mut layout = ui
                    .layout(
                        Grid::columns(5)
                            .spanning()
                            .padding(Sides::all(padding))
                            .gap(gap),
                    )
                    .grow()
                    .style(style)
                    .clip(Clip::Bounds)
                    .open();
                let badge_layer = layout.layer();
                for (index, (label, (rows, columns))) in [
                    ("1", (2, 2)),
                    ("2", (1, 1)),
                    ("3", (1, 2)),
                    ("4", (1, 1)),
                    ("5", (1, 2)),
                    ("6", (2, 1)),
                    ("7", (1, 2)),
                    ("8", (2, 2)),
                    ("9", (1, 1)),
                    ("10", (1, 1)),
                ]
                .into_iter()
                .enumerate()
                {
                    layout.add_span(
                        rows,
                        columns,
                        canvas_item(index, label, badge_layer, config, theme),
                    );
                }
            }
        }
    }
}

fn canvas_items<L: Layout<Item = ()>>(
    layout: &mut UnitScope<'_, L>,
    config: CanvasConfig,
    theme: Config,
) {
    let badge_layer = layout.layer();
    for (index, label) in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]
        .into_iter()
        .enumerate()
    {
        layout.add(canvas_item(index, label, badge_layer, config, theme));
    }
}

fn canvas_item(
    index: usize,
    label: &'static str,
    badge_layer: LayerId,
    config: CanvasConfig,
    theme: Config,
) -> impl Widget<Output = ()> {
    move |ui: &mut Ui| {
        let main = theme.text_xl + (index % 5) as f32 * theme.space_sm;
        let cross = theme.space_xxl * 2.0 + (index % 4) as f32 * theme.space_lg;
        let main = match config.sizing {
            ItemSizing::Fixed => Sizing::fixed(main),
            ItemSizing::Fit => Sizing::fit().min(theme.space_xxl).max(main),
            ItemSizing::Grow => Sizing::grow().min(theme.space_xxl),
        };
        let cross = if config.align == Align::Stretch {
            Sizing::fit()
        } else {
            Sizing::fixed(cross)
        };
        let item = ui.layout(
            Flex::column()
                .align(Align::Stretch)
                .justify(Justify::Center),
        );
        let item = match config.axis {
            Axis::Horizontal => item.width(main).height(cross),
            Axis::Vertical => item.width(cross).height(main),
        };
        let item = if config.transitions {
            item.id(WidgetId::new(("canvas item", index))).transition(
                Transition::new(Duration::from_secs_f32(config.transition_duration / 1000.0))
                    .easing(Easing::EaseOutQuad)
                    .layout(),
            )
        } else {
            item
        };
        let mut rectangle = item
            .style(
                Style::new()
                    .background(colors::ITEMS[index])
                    .uniform_radius(theme.radius_sm),
            )
            .open();
        rectangle.add(
            Text::new(label)
                .color(colors::WHITE)
                .text_size(theme.text_sm)
                .align(HorizontalAlign::Center),
        );
        let anchor = match index {
            0 => Some(Anchor::TopRight),
            4 => Some(Anchor::BottomLeft),
            9 => Some(Anchor::BottomRight),
            _ => None,
        };
        if let Some(anchor) = anchor {
            rectangle.add(|ui: &mut Ui| {
                let mut badge = ui
                    .layout(Flex::row().align(Align::Center).justify(Justify::Center))
                    .fixed(theme.space_xxl, theme.space_lg)
                    .style(
                        Style::new()
                            .background(colors::BACKGROUND)
                            .solid_border(theme.border, colors::WHITE)
                            .uniform_radius(theme.radius_sm),
                    )
                    .layer(badge_layer)
                    .z_index(1)
                    .absolute(Absolute::attach(anchor, Anchor::Center))
                    .open();
                badge.add(
                    Text::new("ABS")
                        .color(colors::WHITE)
                        .text_size(theme.text_sm),
                );
            });
        }
    }
}

blit::builder! {
    struct Button<'a> {
        new(widget_id: WidgetId),
        label: &'a str = "",
        background: blit::color::Color = colors::SURFACE_HIGH,
        clicked_background: blit::color::Color = colors::ACCENT,
        text_color: blit::color::Color = colors::TEXT,
        border_width: f32 = 0.0,
        border_color: blit::color::Color = colors::BORDER,
        radius: f32 = 0.0,
        padding_x: f32 = 8.0,
        padding_y: f32 = 8.0,
        text_size: f32 = 12.0,
        min_width: f32 = 0.0,
        min_height: f32 = 0.0,
    }
}

impl Widget for Button<'_> {
    type Output = bool;

    fn render(self, ui: &mut Ui) -> bool {
        let interaction = ui.interact(self.widget_id, Sense::CLICK);
        let mut button = ui
            .layout(
                Flex::row()
                    .align(Align::Center)
                    .justify(Justify::Center)
                    .padding(Sides {
                        top: self.padding_y,
                        right: self.padding_x,
                        bottom: self.padding_y,
                        left: self.padding_x,
                    }),
            )
            .width(Sizing::fit().min(self.min_width))
            .height(Sizing::fit().min(self.min_height))
            .id(self.widget_id)
            .style(
                Style::new()
                    .background(if interaction.active || interaction.clicked {
                        self.clicked_background
                    } else {
                        self.background
                    })
                    .solid_border(self.border_width, self.border_color)
                    .uniform_radius(self.radius),
            )
            .open();
        button.add(
            Text::new(self.label)
                .color(self.text_color)
                .text_size(self.text_size),
        );
        interaction.clicked
    }
}

fn options<'a, T, I>(
    id: &'a str,
    selected: &'a mut T,
    options: I,
    config: Config,
) -> impl Widget<Output = ()> + 'a
where
    T: Copy + PartialEq + 'a,
    I: IntoIterator<Item = (&'a str, T)> + 'a,
{
    move |ui: &mut Ui| {
        let mut row = ui
            .layout(
                Wrap::horizontal()
                    .item_gap(config.space_md)
                    .run_gap(config.space_md),
            )
            .width(Sizing::grow())
            .open();
        for (index, (label, value)) in options.into_iter().enumerate() {
            if row
                .add(choice((id, index), label, *selected == value, config).min_width(config.sz_md))
            {
                *selected = value;
            }
        }
    }
}

fn choice<'a>(
    id: impl std::hash::Hash,
    label: &'a str,
    selected: bool,
    config: Config,
) -> Button<'a> {
    Button::new(WidgetId::new(id))
        .label(label)
        .background(if selected {
            colors::ACCENT_DARK
        } else {
            colors::SURFACE_HIGH
        })
        .clicked_background(colors::ACCENT)
        .text_color(colors::TEXT)
        .border_width(config.border)
        .border_color(if selected {
            colors::ACCENT
        } else {
            colors::BORDER
        })
        .radius(config.radius_sm)
        .padding_x(config.space_md)
        .padding_y(config.space_sm)
        .text_size(config.text_sm)
        .min_height(config.sz_md)
}

mod colors {
    use blit::color::Color;

    pub const BACKGROUND: Color = Color::from_rgba8(12, 18, 29, 255);
    pub const SURFACE: Color = Color::from_rgba8(20, 29, 45, 255);
    pub const SURFACE_HIGH: Color = Color::from_rgba8(29, 41, 62, 255);
    pub const TRACK: Color = Color::from_rgba8(10, 16, 27, 255);
    pub const CANVAS: Color = Color::from_rgba8(25, 36, 54, 255);
    pub const CANVAS_BORDER: Color = Color::from_rgba8(68, 91, 123, 255);
    pub const GRIP: Color = Color::from_rgba8(46, 77, 101, 255);
    pub const BORDER: Color = Color::from_rgba8(55, 72, 99, 255);
    pub const TEXT: Color = Color::from_rgba8(235, 242, 250, 255);
    pub const TEXT_MUTED: Color = Color::from_rgba8(157, 173, 194, 255);
    pub const TEXT_DIM: Color = Color::from_rgba8(106, 126, 151, 255);
    pub const WHITE: Color = Color::WHITE;
    pub const ACCENT: Color = Color::from_rgba8(91, 220, 185, 255);
    pub const ACCENT_DARK: Color = Color::from_rgba8(31, 111, 104, 255);
    pub const SURFACE_BLUE: Color = Color::from_rgba8(73, 135, 218, 255);
    pub const SURFACE_GREEN: Color = Color::from_rgba8(53, 174, 126, 255);
    pub const SURFACE_ORANGE: Color = Color::from_rgba8(224, 142, 62, 255);
    pub const SURFACE_PURPLE: Color = Color::from_rgba8(146, 92, 220, 255);
    pub const SURFACE_PINK: Color = Color::from_rgba8(218, 89, 143, 255);
    pub const ITEMS: [Color; 10] = [
        SURFACE_BLUE,
        SURFACE_GREEN,
        SURFACE_ORANGE,
        SURFACE_PURPLE,
        SURFACE_PINK,
        Color::from_rgba8(47, 162, 184, 255),
        Color::from_rgba8(111, 148, 68, 255),
        Color::from_rgba8(205, 103, 73, 255),
        Color::from_rgba8(112, 103, 209, 255),
        Color::from_rgba8(191, 79, 119, 255),
    ];
}
