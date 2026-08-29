use std::{fmt::Write, time::Duration};

use blit::{
    Ui,
    animation::{Easing, Transition},
    container::{Absolute, Anchor, LayerId, Sizing, Slot},
    geometry::{LogicalPoint, LogicalSize, Sides},
    image::{
        ImageData, ImageFit, ImageFormat, ImageHandle, ImagePixels, ImageSampling, ImageTiling,
    },
    input::{Input, Key},
    interact::{Interaction, Sense, WidgetId},
    layout::{
        Align, Axis, Constraints, Flex, Grid, ItemScope, Justify, Layout, LayoutCx, UnitScope, Wrap,
    },
    style::{Clip, Style},
    text::{HorizontalAlign, TextWrap},
    widget::{Image, Rectangle, ScrollArea, ScrollState, Text, TextInput, TextInputState, Widget},
};

pub struct Showcase {
    canvas: CanvasConfig,
    transition_easing: Easing,
    transition_target: bool,
    carousel: usize,
    resize: ResizeState,
    page: Page,
    scroll: ScrollState,
    name: TextInputState,
    password: TextInputState,
    image: Option<ImageHandle>,
    last_frame: Option<Duration>,
    fps_label: String,
}

#[derive(Clone, Copy)]
struct CanvasConfig {
    layout: CanvasLayout,
    axis: Axis,
    justify: Justify,
    align: Align,
    sizing: ItemSizing,
    zoom: f32,
    gap: f32,
    padding: f32,
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

impl Default for CanvasConfig {
    fn default() -> Self {
        Self {
            layout: CanvasLayout::Flex,
            axis: Axis::Horizontal,
            justify: Justify::Start,
            align: Align::Center,
            sizing: ItemSizing::Fixed,
            zoom: 1.0,
            gap: 8.0,
            padding: 8.0,
            transitions: true,
            transition_duration: 300.0,
        }
    }
}

impl Default for Showcase {
    fn default() -> Self {
        Self {
            canvas: CanvasConfig::default(),
            transition_easing: Easing::EaseInOutQuad,
            transition_target: false,
            carousel: 0,
            resize: ResizeState::default(),
            page: Page::Layout,
            scroll: ScrollState::default(),
            name: TextInputState::default(),
            password: TextInputState::default(),
            image: None,
            last_frame: None,
            fps_label: "FPS --".into(),
        }
    }
}

impl Showcase {
    pub fn set_page(&mut self, page: Page) {
        self.page = page;
    }

    pub fn render(&mut self, ui: &mut Ui) {
        let scale_factor = match ui.input() {
            Input::Key(key) if key.pressed && key.modifiers.control() => match key.key {
                Key::Character('+') | Key::Character('=') => Some(ui.scale_factor() + 0.25),
                Key::Character('-') => Some(ui.scale_factor() - 0.25),
                _ => None,
            },
            _ => None,
        };
        if let Some(scale_factor) = scale_factor {
            ui.set_scale_factor(scale_factor.clamp(0.5, 4.0));
        }

        let now = ui.time();
        if let Some(previous) = self.last_frame.replace(now) {
            let elapsed = now.saturating_sub(previous).as_secs_f32();
            if elapsed > 0.0 {
                self.fps_label.clear();
                let _ = write!(self.fps_label, "FPS {:.0}", 1.0 / elapsed);
            }
        }

        ui.clear();
        let screen = ui.screen();
        let max_width = (screen.width - 420.0).max(240.0);
        let max_height = (screen.height - 190.0).max(180.0);
        let mut root = ui
            .layout(Flex::column().padding(Sides::all(20.0)).gap(16.0))
            .grow()
            .style(Style::new().background(colors::BACKGROUND))
            .open();

        root.add(|ui: &mut Ui| self.header(ui));
        root.add(|ui: &mut Ui| match self.page {
            Page::Layout => self.body(ui, max_width, max_height),
            Page::Scrolling => self.scrolling(ui),
            Page::Input => self.input_page(ui),
            Page::Images => self.images(ui),
            Page::Animation => self.animation_page(ui),
        });
        root.add(|ui: &mut Ui| Self::screen_badge(ui, &self.fps_label));
    }
}

impl Showcase {
    fn header(&mut self, ui: &mut Ui) {
        let mut header = ui
            .layout(
                Flex::row()
                    .align(Align::Center)
                    .justify(Justify::SpaceBetween),
            )
            .width(Sizing::grow())
            .height(Sizing::fit().min(64.0))
            .style(Style::new().background(colors::BACKGROUND))
            .open();
        header.add(|ui: &mut Ui| {
            let mut title = ui.layout(Flex::column().gap(3.0)).open();
            title.add(
                Text::new("BLIT / SHOWCASE")
                    .color(colors::TEXT)
                    .text_size(23.0),
            );
            title.add(
                Text::new(
                    "Explore Blit's layout, input, rendering, scrolling, and animation primitives",
                )
                .color(colors::TEXT_MUTED)
                .text_size(12.0),
            );
        });
        header.add(|ui: &mut Ui| {
            let mut tabs = ui
                .layout(Flex::row().padding(Sides::all(4.0)).gap(2.0))
                .style(Style::new().background(colors::TRACK).uniform_radius(9.0))
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
                if tabs
                    .add(
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
                            .border_width(1.0)
                            .border_color(if selected {
                                colors::ACCENT
                            } else {
                                colors::TRACK
                            })
                            .radius(6.0)
                            .padding_x(15.0)
                            .padding_y(9.0)
                            .text_size(12.0),
                    )
                    .clicked()
                {
                    self.page = page;
                }
            }
        });
        if header
            .add(choice("reset playground", "Reset", false).padding_x(14.0))
            .clicked()
        {
            self.canvas = CanvasConfig::default();
            self.transition_easing = Easing::EaseInOutQuad;
            self.transition_target = false;
            self.carousel = 0;
            self.resize.reset();
        }
    }

    fn body(&mut self, ui: &mut Ui, max_width: f32, max_height: f32) {
        let mut body = ui
            .layout(Flex::row().gap(16.0))
            .grow()
            .clip(Clip::Bounds)
            .open();

        body.add(|ui: &mut Ui| self.controls(ui));
        body.add(|ui: &mut Ui| self.preview(ui, max_width, max_height));
    }

    fn controls(&mut self, ui: &mut Ui) {
        let mut controls = ui
            .layout(Flex::column().padding(Sides::all(15.0)).gap(11.0))
            .width(Sizing::fixed(310.0))
            .height(Sizing::grow())
            .style(
                Style::new()
                    .background(colors::SURFACE)
                    .solid_border(1.0, colors::BORDER)
                    .uniform_radius(11.0),
            )
            .clip(Clip::Bounds)
            .open();
        controls.add(
            Text::new("LAYOUT PARAMETERS")
                .color(colors::ACCENT)
                .text_size(12.0),
        );
        controls.add(
            Text::new("layout")
                .color(colors::TEXT_MUTED)
                .text_size(11.0),
        );
        controls.add(options(
            "layout",
            &mut self.canvas.layout,
            [
                ("Flex", CanvasLayout::Flex),
                ("Wrap", CanvasLayout::Wrap),
                ("Grid", CanvasLayout::Grid),
            ],
            6.0,
            12.0,
        ));

        controls.add(
            Text::new("item transitions")
                .color(colors::TEXT_MUTED)
                .text_size(11.0),
        );
        controls.add(options(
            "item transitions",
            &mut self.canvas.transitions,
            [("On", true), ("Off", false)],
            6.0,
            10.0,
        ));
        controls.add(
            Text::new("transition time")
                .color(colors::TEXT_MUTED)
                .text_size(11.0),
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
            5.0,
            8.0,
        ));

        controls.add(Text::new("axis").color(colors::TEXT_MUTED).text_size(11.0));
        controls.add(options(
            "axis",
            &mut self.canvas.axis,
            [
                ("Horizontal", Axis::Horizontal),
                ("Vertical", Axis::Vertical),
            ],
            6.0,
            10.0,
        ));

        controls.add(
            Text::new("justify")
                .color(colors::TEXT_MUTED)
                .text_size(11.0),
        );
        controls.add(options(
            "justify start",
            &mut self.canvas.justify,
            [
                ("Start", Justify::Start),
                ("Center", Justify::Center),
                ("End", Justify::End),
            ],
            6.0,
            7.0,
        ));
        controls.add(options(
            "justify space",
            &mut self.canvas.justify,
            [
                ("Between", Justify::SpaceBetween),
                ("Around", Justify::SpaceAround),
                ("Evenly", Justify::SpaceEvenly),
            ],
            6.0,
            7.0,
        ));

        controls.add(Text::new("align").color(colors::TEXT_MUTED).text_size(11.0));
        controls.add(options(
            "align",
            &mut self.canvas.align,
            [
                ("Start", Align::Start),
                ("Center", Align::Center),
                ("End", Align::End),
                ("Stretch", Align::Stretch),
            ],
            5.0,
            6.0,
        ));

        controls.add(
            Text::new("item sizing")
                .color(colors::TEXT_MUTED)
                .text_size(11.0),
        );
        controls.add(options(
            "sizing",
            &mut self.canvas.sizing,
            [
                ("Fixed", ItemSizing::Fixed),
                ("Fit", ItemSizing::Fit),
                ("Grow", ItemSizing::Grow),
            ],
            6.0,
            10.0,
        ));

        controls.add(Text::new("zoom").color(colors::TEXT_MUTED).text_size(11.0));
        controls.add(options(
            "zoom",
            &mut self.canvas.zoom,
            [
                ("50%", 0.5),
                ("75%", 0.75),
                ("100%", 1.0),
                ("125%", 1.25),
                ("150%", 1.5),
            ],
            5.0,
            5.0,
        ));

        controls.add(Text::new("gap").color(colors::TEXT_MUTED).text_size(11.0));
        controls.add(options(
            "gap",
            &mut self.canvas.gap,
            [
                ("0", 0.0),
                ("4", 4.0),
                ("8", 8.0),
                ("16", 16.0),
                ("24", 24.0),
            ],
            5.0,
            9.0,
        ));

        controls.add(
            Text::new("padding")
                .color(colors::TEXT_MUTED)
                .text_size(11.0),
        );
        controls.add(options(
            "padding",
            &mut self.canvas.padding,
            [
                ("0", 0.0),
                ("4", 4.0),
                ("8", 8.0),
                ("16", 16.0),
                ("24", 24.0),
            ],
            5.0,
            9.0,
        ));

        controls.add(
            Text::new("Flex distributes free space, Wrap forms runs, and Grid automatically packs row and column spans into five equal columns. Axis, alignment, and justification apply to the flow layouts.")
                .color(colors::TEXT_DIM)
                .text_size(11.0)
                .wrap(TextWrap::Word),
        );
    }

    fn preview(&mut self, ui: &mut Ui, max_width: f32, max_height: f32) {
        let mut preview = ui
            .layout(Flex::column().padding(Sides::all(12.0)).gap(9.0))
            .grow()
            .style(
                Style::new()
                    .background(colors::SURFACE)
                    .solid_border(1.0, colors::BORDER)
                    .uniform_radius(11.0),
            )
            .open();
        preview.add(
            Text::new("DRAG THE RIGHT EDGE, BOTTOM EDGE, OR CORNER • ZOOM SCALES CONTENT")
                .color(colors::TEXT_DIM)
                .text_size(10.0),
        );

        preview.add(|ui: &mut Ui| self.playground(ui, max_width, max_height));
        self.carousel = preview.add(carousel(self.carousel));
        preview.add(|ui: &mut Ui| self.transitions(ui));
    }

    fn playground(&mut self, ui: &mut Ui, max_width: f32, max_height: f32) {
        let mut viewport = ui
            .layout(Flex::column().padding(Sides::all(8.0)))
            .grow()
            .style(Style::new().background(colors::TRACK).uniform_radius(8.0))
            .clip(Clip::Bounds)
            .open();
        viewport.add(
            Resizable::new(&mut self.resize)
                .id("playground canvas")
                .width(Sizing::percent(0.85))
                .height(Sizing::percent(0.8))
                .min_size(240.0, 180.0)
                .max_size(max_width, max_height)
                .content(canvas(self.canvas)),
        );
    }
    fn transitions(&mut self, ui: &mut Ui) {
        let mut transitions = ui
            .layout(Flex::column().padding(Sides::all(10.0)).gap(8.0))
            .width(Sizing::grow())
            .height(Sizing::fixed(156.0))
            .style(
                Style::new()
                    .background(colors::TRACK)
                    .solid_border(1.0, colors::BORDER)
                    .uniform_radius(8.0),
            )
            .open();
        transitions.add(|ui: &mut Ui| self.transition_controls(ui));
        transitions.add(|ui: &mut Ui| self.transition_tracks(ui, self.transition_easing));
    }

    fn transition_controls(&mut self, ui: &mut Ui) {
        let mut header = ui
            .layout(Flex::row().align(Align::Center).gap(6.0))
            .width(Sizing::grow())
            .open();
        header.add(
            Text::new("TRANSITIONS")
                .color(colors::ACCENT)
                .text_size(10.0),
        );
        header.add(|ui: &mut Ui| {
            let mut choices = ui
                .layout(Flex::row().justify(Justify::End).gap(4.0))
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
                if choices
                    .add(
                        choice(
                            ("transition easing", index),
                            label,
                            self.transition_easing == easing,
                        )
                        .padding_x(7.0)
                        .padding_y(4.0),
                    )
                    .clicked()
                {
                    self.transition_easing = easing;
                }
            }
        });
        if header
            .add(
                choice("reverse transitions", "Reverse", self.transition_target)
                    .padding_x(12.0)
                    .padding_y(4.0),
            )
            .clicked()
        {
            self.transition_target = !self.transition_target;
        }
    }

    fn transition_tracks(&self, ui: &mut Ui, easing: Easing) {
        let mut tracks = ui.layout(Flex::row().gap(8.0)).grow().open();
        tracks.add(|ui: &mut Ui| {
            let mut track = ui
                .layout(Flex::row().padding(Sides::all(7.0)))
                .width(Sizing::percent(0.5))
                .height(Sizing::grow())
                .style(Style::new().background(colors::SURFACE).uniform_radius(6.0))
                .clip(Clip::Bounds)
                .open();
            track.add(|ui: &mut Ui| {
                let mut specimen = ui
                    .layout(Flex::row().align(Align::Center).justify(Justify::Center))
                    .fixed(76.0, 30.0)
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
                            .uniform_radius(5.0),
                    )
                    .open();
                specimen.add(Text::new("X / Y").color(colors::WHITE).text_size(10.0));
            });
        });
        tracks.add(|ui: &mut Ui| {
            let mut track = ui
                .layout(Flex::row().padding(Sides::all(7.0)))
                .width(Sizing::percent(0.5))
                .height(Sizing::grow())
                .style(Style::new().background(colors::SURFACE).uniform_radius(6.0))
                .clip(Clip::Bounds)
                .open();
            let (width, height) = if self.transition_target {
                (132.0, 50.0)
            } else {
                (72.0, 28.0)
            };
            track.add(|ui: &mut Ui| {
                let mut specimen = ui
                    .layout(Flex::row().align(Align::Center).justify(Justify::Center))
                    .fixed(width, height)
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
                            .uniform_radius(5.0),
                    )
                    .open();
                specimen.add(Text::new("W / H").color(colors::WHITE).text_size(10.0));
            });
        });
    }
    fn scrolling(&mut self, ui: &mut Ui) {
        let mut page = ui
            .layout(Flex::column().padding(Sides::all(16.0)).gap(10.0))
            .grow()
            .style(
                Style::new()
                    .background(colors::SURFACE)
                    .solid_border(1.0, colors::BORDER)
                    .uniform_radius(11.0),
            )
            .open();
        page.add(Text::new("SCROLLING").color(colors::ACCENT).text_size(13.0));
        page.add(Text::new("Use the wheel or drag the list. The viewport clips overflowing content and retains its offset.").color(colors::TEXT_MUTED).text_size(12.0));
        page.add(|ui: &mut Ui| {
            let mut scroll = ScrollArea::vertical(&mut self.scroll)
                .id("showcase scroll")
                .padding(Sides::all(10.0))
                .gap(8.0)
                .begin(ui);
            for index in 0..24 {
                scroll.add(|ui: &mut Ui| {
                    let mut row = ui
                        .layout(
                            Flex::row()
                                .padding(Sides::all(12.0))
                                .align(Align::Center)
                                .gap(12.0),
                        )
                        .width(Sizing::grow())
                        .height(Sizing::fixed(58.0))
                        .style(
                            Style::new()
                                .background(if index % 2 == 0 {
                                    colors::SURFACE_HIGH
                                } else {
                                    colors::CANVAS
                                })
                                .uniform_radius(7.0),
                        )
                        .open();
                    row.add(Text::new("ITEM").color(colors::WHITE).text_size(16.0));
                    row.add(
                        Text::new("scrollable content")
                            .color(colors::WHITE)
                            .text_size(12.0),
                    );
                });
            }
        });
    }

    fn input_page(&mut self, ui: &mut Ui) {
        let mut page = ui
            .layout(Flex::column().padding(Sides::all(16.0)).gap(14.0))
            .grow()
            .style(
                Style::new()
                    .background(colors::SURFACE)
                    .solid_border(1.0, colors::BORDER)
                    .uniform_radius(11.0),
            )
            .open();
        page.add(
            Text::new("TEXT INPUT / FOCUS")
                .color(colors::ACCENT)
                .text_size(13.0),
        );
        page.add(
            Text::new("Click a field, type, use arrow keys, shift to select, and enter to accept.")
                .color(colors::TEXT_MUTED)
                .text_size(12.0),
        );
        let name_border = if self.name.focused {
            colors::ACCENT
        } else {
            colors::BORDER
        };
        page.add(
            TextInput::new(&mut self.name)
                .slot(Slot::new().width(Sizing::fixed(360.0)))
                .padding(Sides::all(10.0))
                .style(
                    Style::new()
                        .background(colors::TRACK)
                        .solid_border(1.0, name_border)
                        .uniform_radius(7.0),
                )
                .text_color(colors::TEXT)
                .cursor_color(colors::ACCENT)
                .selection_background(colors::ACCENT_DARK)
                .text_size(15.0),
        );
        let password_border = if self.password.focused {
            colors::ACCENT
        } else {
            colors::BORDER
        };
        page.add(
            TextInput::new(&mut self.password)
                .slot(Slot::new().width(Sizing::fixed(360.0)))
                .padding(Sides::all(10.0))
                .style(
                    Style::new()
                        .background(colors::TRACK)
                        .solid_border(1.0, password_border)
                        .uniform_radius(7.0),
                )
                .mask('●')
                .text_color(colors::TEXT)
                .cursor_color(colors::ACCENT)
                .selection_background(colors::ACCENT_DARK)
                .text_size(15.0),
        );
    }

    fn images(&mut self, ui: &mut Ui) {
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
            .layout(Flex::column().padding(Sides::all(16.0)).gap(12.0))
            .grow()
            .style(
                Style::new()
                    .background(colors::SURFACE)
                    .solid_border(1.0, colors::BORDER)
                    .uniform_radius(11.0),
            )
            .open();
        page.add(
            Text::new("IMAGE RENDERING")
                .color(colors::ACCENT)
                .text_size(13.0),
        );
        page.add(
            Text::new(
                "The same RGBA image rendered with nearest, bilinear, cover, and repeat settings.",
            )
            .color(colors::TEXT_MUTED)
            .text_size(12.0),
        );
        page.add(|ui: &mut Ui| {
            let mut samples = ui.layout(Flex::row().gap(12.0)).grow().open();
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
                        .layout(Flex::column().gap(6.0))
                        .width(Sizing::percent(0.25))
                        .open();
                    sample.add(Text::new(label).color(colors::TEXT_DIM).text_size(10.0));
                    sample.add(
                        Image::new(image)
                            .slot(
                                Slot::new()
                                    .width(Sizing::grow())
                                    .height(Sizing::fixed(180.0)),
                            )
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
            .layout(Flex::column().padding(Sides::all(16.0)).gap(14.0))
            .grow()
            .style(
                Style::new()
                    .background(colors::SURFACE)
                    .solid_border(1.0, colors::BORDER)
                    .uniform_radius(11.0),
            )
            .open();
        page.add(
            Text::new("KEYED LOOPING ANIMATIONS")
                .color(colors::ACCENT)
                .text_size(13.0),
        );
        page.add(
            Text::new("These values come from Ui::animate_loop, rather than layout transitions.")
                .color(colors::TEXT_MUTED)
                .text_size(12.0),
        );
        page.add(|ui: &mut Ui| {
            let mut track = ui
                .layout(Flex::row())
                .width(Sizing::grow())
                .height(Sizing::fixed(80.0))
                .style(Style::new().background(colors::TRACK).uniform_radius(8.0))
                .open();
            track.add(|ui: &mut Ui| {
                ui.layout(Flex::row())
                    .fixed(50.0, 50.0)
                    .absolute(
                        Absolute::attach(Anchor::Left, Anchor::Left)
                            .offset(loop_value * 500.0, 0.0),
                    )
                    .style(
                        Style::new()
                            .background(colors::SURFACE_GREEN)
                            .uniform_radius(25.0),
                    )
                    .open();
            });
        });
        page.add(|ui: &mut Ui| {
            let size = 30.0 + pulse * 70.0;
            let mut pulse_box = ui
                .layout(Flex::row().align(Align::Center).justify(Justify::Center))
                .width(Sizing::grow())
                .height(Sizing::fixed(130.0))
                .style(Style::new().background(colors::TRACK).uniform_radius(8.0))
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

    fn screen_badge(ui: &mut Ui, fps: &str) {
        let mut screen_badge = ui
            .layout(
                Flex::row()
                    .padding(Sides {
                        top: 6.0,
                        right: 10.0,
                        bottom: 6.0,
                        left: 10.0,
                    })
                    .gap(6.0)
                    .align(Align::Center),
            )
            .style(
                Style::new()
                    .background(colors::SURFACE_HIGH)
                    .solid_border(1.0, colors::ACCENT)
                    .uniform_radius(7.0),
            )
            .z_index(20)
            .absolute(
                Absolute::screen(0.0, 0.0)
                    .anchors(Anchor::BottomRight, Anchor::BottomRight)
                    .offset(-16.0, -16.0),
            )
            .open();
        screen_badge.add(
            Rectangle::new()
                .slot(Slot::new().fixed(6.0, 6.0))
                .style(Style::new().background(colors::ACCENT).uniform_radius(3.0)),
        );
        screen_badge.add(Text::new(fps).color(colors::TEXT).text_size(10.0));
        screen_badge.add(
            Text::new("SCREEN ABSOLUTE")
                .color(colors::TEXT_DIM)
                .text_size(10.0),
        );
    }
}

#[derive(Clone, Copy)]
struct CarouselLayout {
    spacing: f32,
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
            natural.width = natural.width.max(child.width);
            natural.height = natural.height.max(child.height);
        }
        natural.width += self.spacing * 2.0;
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
                width: cx.sizing(node, Axis::Horizontal).resolve(
                    cx.size(node).width,
                    size.width,
                    true,
                ),
                height: cx.sizing(node, Axis::Vertical).resolve(
                    cx.size(node).height,
                    size.height,
                    true,
                ),
            };
            cx.constrain_child(node, Constraints::tight(child));
            let distance = forward.min(backward);
            cx.set_z_index(node, -i16::try_from(distance).unwrap_or(i16::MAX));
            let offset = if forward <= backward {
                forward as f32
            } else {
                -(backward as f32)
            } * self.spacing;
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

fn carousel(mut active: usize) -> impl Widget<Output = usize> {
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
            .layout(Flex::column().gap(7.0))
            .width(Sizing::grow())
            .height(Sizing::fixed(174.0))
            .open();
        section.add(|ui: &mut Ui| {
            let mut controls = ui
                .layout(Flex::row().align(Align::Center).gap(5.0))
                .width(Sizing::grow())
                .open();
            controls.add(
                Text::new("CAROUSEL / CUSTOM OVERLAP LAYOUT")
                    .color(colors::ACCENT)
                    .text_size(10.0),
            );
            controls.add(Rectangle::new().slot(Slot::new().width(Sizing::grow())));
            if controls
                .add(choice("carousel previous", "PREV", false).padding_y(4.0))
                .clicked()
            {
                active = (active + CARDS.len() - 1) % CARDS.len();
            }
            if controls
                .add(choice("carousel next", "NEXT", false).padding_y(4.0))
                .clicked()
            {
                active = (active + 1) % CARDS.len();
            }
        });
        section.add(|ui: &mut Ui| {
            let mut cards = ui
                .layout(CarouselLayout {
                    spacing: 112.0,
                    active,
                })
                .grow()
                .style(
                    Style::new()
                        .background(colors::TRACK)
                        .solid_border(1.0, colors::BORDER)
                        .uniform_radius(8.0),
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
                                .padding(Sides::all(12.0))
                                .justify(Justify::SpaceBetween),
                        )
                        .fixed(170.0, 104.0)
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
                                    if index == active { 2.0 } else { 1.0 },
                                    if index == active {
                                        colors::WHITE
                                    } else {
                                        colors::BORDER
                                    },
                                )
                                .uniform_radius(9.0),
                        )
                        .open();
                    card.add(Text::new(number).color(colors::WHITE).text_size(22.0));
                    card.add(Text::new(label).color(colors::WHITE).text_size(10.0));
                });
            }
        });
        active
    }
}

#[derive(Default)]
struct ResizeState {
    size: Option<LogicalSize>,
}

impl ResizeState {
    fn reset(&mut self) {
        self.size = None;
    }
}

blit::builder! {
    struct Resizable<'a> {
        new(state: &'a mut ResizeState),
        widget_id: WidgetId = WidgetId::new("resizable"),
        width: Sizing = Sizing::grow(),
        height: Sizing = Sizing::grow(),
        minimum: LogicalSize = LogicalSize::default(),
        maximum: LogicalSize = LogicalSize {
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
    fn id(mut self, source: impl std::hash::Hash) -> Self {
        self.widget_id = WidgetId::new(source);
        self
    }

    fn min_size(mut self, width: f32, height: f32) -> Self {
        self.minimum = LogicalSize { width, height };
        self
    }

    fn max_size(mut self, width: f32, height: f32) -> Self {
        self.maximum = LogicalSize { width, height };
        self
    }

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
        let right_id = config.widget_id.child("right grip");
        let bottom_id = config.widget_id.child("bottom grip");
        let corner_id = config.widget_id.child("corner grip");
        let right = ui.interact(right_id, Sense::DRAG);
        let bottom = ui.interact(bottom_id, Sense::DRAG);
        let corner = ui.interact(corner_id, Sense::DRAG);
        let delta = LogicalPoint {
            x: right.drag_delta.x + corner.drag_delta.x,
            y: bottom.drag_delta.y + corner.drag_delta.y,
        };
        let max_size = LogicalSize {
            width: config.maximum.width.max(config.minimum.width),
            height: config.maximum.height.max(config.minimum.height),
        };
        if delta != LogicalPoint::default() {
            let mut size = config.state.size.unwrap_or_else(|| {
                ui.geometry(config.widget_id)
                    .map_or(config.minimum, |area| LogicalSize {
                        width: area.width,
                        height: area.height,
                    })
            });
            size.width += delta.x;
            size.height += delta.y;
            config.state.size = Some(size);
        }
        if let Some(size) = &mut config.state.size {
            size.width = size.width.clamp(config.minimum.width, max_size.width);
            size.height = size.height.clamp(config.minimum.height, max_size.height);
        }

        let shell = ui.layout(Flex::column()).id(config.widget_id);
        let shell = if let Some(size) = config.state.size {
            shell.fixed(size.width, size.height)
        } else {
            shell.width(config.width).height(config.height)
        };
        let mut shell = shell.open();
        shell.add(self.content);
        shell.add(ResizeGrip {
            id: right_id,
            interaction: right,
            edge: ResizeEdge::Right,
        });
        shell.add(ResizeGrip {
            id: bottom_id,
            interaction: bottom,
            edge: ResizeEdge::Bottom,
        });
        shell.add(ResizeGrip {
            id: corner_id,
            interaction: corner,
            edge: ResizeEdge::Corner,
        });
    }
}

struct ResizeGrip {
    id: WidgetId,
    interaction: Interaction,
    edge: ResizeEdge,
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
        let (layout, width, height, absolute, z_index, marker_width, marker_height, color) =
            match self.edge {
                ResizeEdge::Right => (
                    Flex::column().align(Align::Center).justify(Justify::Center),
                    Sizing::fixed(12.0),
                    Sizing::percent(1.0),
                    Absolute::attach(Anchor::Right, Anchor::Left),
                    1,
                    3.0,
                    48.0,
                    colors::GRIP,
                ),
                ResizeEdge::Bottom => (
                    Flex::row().align(Align::Center).justify(Justify::Center),
                    Sizing::percent(1.0),
                    Sizing::fixed(12.0),
                    Absolute::attach(Anchor::Bottom, Anchor::Top),
                    1,
                    48.0,
                    3.0,
                    colors::GRIP,
                ),
                ResizeEdge::Corner => (
                    Flex::row().align(Align::Center).justify(Justify::Center),
                    Sizing::fixed(12.0),
                    Sizing::fixed(12.0),
                    Absolute::attach(Anchor::BottomRight, Anchor::TopLeft),
                    2,
                    6.0,
                    6.0,
                    colors::GRIP_CORNER,
                ),
            };
        let mut grip = ui
            .layout(layout)
            .width(width)
            .height(height)
            .id(self.id)
            .z_index(z_index)
            .absolute(absolute)
            .open();
        grip.add(
            Rectangle::new()
                .slot(Slot::new().fixed(marker_width, marker_height))
                .style(
                    Style::new()
                        .background(if self.interaction.hovered || self.interaction.dragging {
                            colors::ACCENT
                        } else {
                            color
                        })
                        .uniform_radius(marker_width.min(marker_height) / 2.0),
                ),
        );
        if matches!(self.edge, ResizeEdge::Right)
            && (self.interaction.hovered || self.interaction.dragging)
        {
            grip.add(|ui: &mut Ui| {
                let mut readout = ui
                    .layout(Flex::row().padding(Sides {
                        top: 4.0,
                        right: 7.0,
                        bottom: 4.0,
                        left: 7.0,
                    }))
                    .style(
                        Style::new()
                            .background(colors::BACKGROUND)
                            .solid_border(1.0, colors::ACCENT)
                            .uniform_radius(5.0),
                    )
                    .z_index(10)
                    .absolute(Absolute::attach(Anchor::Left, Anchor::Right).offset(-8.0, 0.0))
                    .open();
                readout.add(Text::new("DRAG X").color(colors::TEXT).text_size(9.0));
            });
        }
    }
}

fn canvas(config: CanvasConfig) -> impl Widget<Output = ()> {
    move |ui: &mut Ui| {
        let style = Style::new()
            .background(colors::CANVAS)
            .solid_border(2.0, colors::CANVAS_BORDER)
            .uniform_radius(8.0);
        match config.layout {
            CanvasLayout::Flex => {
                let mut layout = ui
                    .layout(
                        Flex::new(config.axis)
                            .padding(Sides::all(config.padding * config.zoom))
                            .gap(config.gap * config.zoom)
                            .align(config.align)
                            .justify(config.justify),
                    )
                    .grow()
                    .style(style)
                    .clip(Clip::Bounds)
                    .open();
                canvas_items(&mut layout, config);
            }
            CanvasLayout::Wrap => {
                let mut layout = ui
                    .layout(
                        Wrap::new(config.axis)
                            .padding(Sides::all(config.padding * config.zoom))
                            .gap(config.gap * config.zoom)
                            .align(config.align)
                            .justify(config.justify),
                    )
                    .grow()
                    .style(style)
                    .clip(Clip::Bounds)
                    .open();
                canvas_items(&mut layout, config);
            }
            CanvasLayout::Grid => {
                let mut layout = ui
                    .layout(
                        Grid::columns(5)
                            .spanning()
                            .padding(Sides::all(config.padding * config.zoom))
                            .gap(config.gap * config.zoom),
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
                        canvas_item(index, label, badge_layer, config),
                    );
                }
            }
        }
    }
}

fn canvas_items<L: Layout<Item = ()>>(layout: &mut UnitScope<'_, L>, config: CanvasConfig) {
    let badge_layer = layout.layer();
    for (index, label) in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]
        .into_iter()
        .enumerate()
    {
        layout.add(canvas_item(index, label, badge_layer, config));
    }
}

fn canvas_item(
    index: usize,
    label: &'static str,
    badge_layer: LayerId,
    config: CanvasConfig,
) -> impl Widget<Output = ()> {
    move |ui: &mut Ui| {
        let main = (26.0 + (index % 5) as f32 * 5.0) * config.zoom;
        let cross = (48.0 + (index % 4) as f32 * 13.0) * config.zoom;
        let main = match config.sizing {
            ItemSizing::Fixed => Sizing::fixed(main),
            ItemSizing::Fit => Sizing::fit().min(20.0 * config.zoom).max(main),
            ItemSizing::Grow => Sizing::grow().min(20.0 * config.zoom),
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
                    .uniform_radius(5.0),
            )
            .open();
        rectangle.add(
            Text::new(label)
                .color(colors::WHITE)
                .text_size(11.0 * config.zoom)
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
                    .fixed(
                        (28.0 * config.zoom).max(20.0),
                        (14.0 * config.zoom).max(10.0),
                    )
                    .style(
                        Style::new()
                            .background(colors::BACKGROUND)
                            .solid_border(1.0, colors::WHITE)
                            .uniform_radius(5.0),
                    )
                    .layer(badge_layer)
                    .z_index(1)
                    .absolute(Absolute::attach(anchor, Anchor::Center))
                    .open();
                badge.add(
                    Text::new("ABS")
                        .color(colors::WHITE)
                        .text_size((8.0 * config.zoom).max(7.0)),
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
    }
}

struct ButtonResponse(bool);

impl ButtonResponse {
    fn clicked(self) -> bool {
        self.0
    }
}

impl Widget for Button<'_> {
    type Output = ButtonResponse;

    fn render(self, ui: &mut Ui) -> ButtonResponse {
        let interaction = ui.interact(self.widget_id, Sense::CLICK);
        let mut button = ui
            .layout(Flex::row().padding(Sides {
                top: self.padding_y,
                right: self.padding_x,
                bottom: self.padding_y,
                left: self.padding_x,
            }))
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
        ButtonResponse(interaction.clicked)
    }
}

fn options<'a, T, I>(
    id: &'a str,
    selected: &'a mut T,
    options: I,
    gap: f32,
    padding_x: f32,
) -> impl Widget<Output = ()> + 'a
where
    T: Copy + PartialEq + 'a,
    I: IntoIterator<Item = (&'a str, T)> + 'a,
{
    move |ui: &mut Ui| {
        let mut row = ui.layout(Flex::row().gap(gap)).width(Sizing::grow()).open();
        for (index, (label, value)) in options.into_iter().enumerate() {
            if row
                .add(choice((id, index), label, *selected == value).padding_x(padding_x))
                .clicked()
            {
                *selected = value;
            }
        }
    }
}

fn choice<'a>(id: impl std::hash::Hash, label: &'a str, selected: bool) -> Button<'a> {
    Button::new(WidgetId::new(id))
        .label(label)
        .background(if selected {
            colors::ACCENT_DARK
        } else {
            colors::SURFACE_HIGH
        })
        .clicked_background(colors::ACCENT)
        .text_color(colors::TEXT)
        .border_width(1.0)
        .border_color(if selected {
            colors::ACCENT
        } else {
            colors::BORDER
        })
        .radius(6.0)
        .padding_x(10.0)
        .padding_y(6.0)
        .text_size(10.0)
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
    pub const GRIP_CORNER: Color = Color::from_rgba8(63, 103, 128, 255);
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
