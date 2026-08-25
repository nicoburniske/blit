use std::time::Duration;

use blit::{
    Ui,
    animation::{Easing, Transition},
    container::{Absolute, Anchor, Sizing},
    geometry::{LogicalInsets, LogicalPoint, LogicalSize},
    input::{Input, Key},
    interact::{Interaction, Sense, WidgetId},
    layout::{Align, Axis, Flex, Justify},
    style::{Clip, Style},
    text::{FontId, HorizontalAlign, TextWrap},
    widget::{Rectangle, Text, Widget},
};
use blit_cpu::{Font, FontFace, RendererConfig};
use blit_desktop::{Application, Config, EventLoopProxy, Root};

fn main() {
    blit_desktop::run::<State>(Config {
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

struct State {
    canvas: CanvasConfig,
    transition_easing: Easing,
    transition_target: bool,
    resize: ResizeState,
}

#[derive(Clone, Copy)]
struct CanvasConfig {
    axis: Axis,
    justify: Justify,
    align: Align,
    sizing: ItemSizing,
    zoom: f32,
    gap: f32,
    padding: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemSizing {
    Fixed,
    Fit,
    Grow,
}

impl Default for CanvasConfig {
    fn default() -> Self {
        Self {
            axis: Axis::Horizontal,
            justify: Justify::Start,
            align: Align::Center,
            sizing: ItemSizing::Fixed,
            zoom: 1.0,
            gap: 8.0,
            padding: 8.0,
        }
    }
}

impl Application for State {
    type Input = ();

    fn new(_: EventLoopProxy<Self::Input>, _: Root<Self>) -> Self {
        Self {
            canvas: CanvasConfig::default(),
            transition_easing: Easing::EaseInOutQuad,
            transition_target: false,
            resize: ResizeState::default(),
        }
    }

    fn input(&mut self, _: Self::Input) {}

    fn render(&mut self, ui: &mut Ui) {
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

        ui.clear();
        let screen = ui.screen();
        let max_width = (screen.width - 420.0).max(240.0);
        let max_height = (screen.height - 190.0).max(180.0);
        let mut root = ui
            .layout(
                Flex::column()
                    .padding(LogicalInsets::uniform(20.0))
                    .gap(16.0),
            )
            .grow()
            .background(colors::BACKGROUND)
            .open();

        root.add(|ui: &mut Ui| self.header(ui));
        root.add(|ui: &mut Ui| self.body(ui, max_width, max_height));
        root.add(Self::screen_badge);
    }
}

impl State {
    fn header(&mut self, ui: &mut Ui) {
        let mut header = ui
            .layout(
                Flex::row()
                    .align(Align::Center)
                    .justify(Justify::SpaceBetween),
            )
            .width(Sizing::grow())
            .background(colors::BACKGROUND)
            .open();
        header.add(|ui: &mut Ui| {
            let mut title = ui.layout(Flex::column().gap(3.0)).open();
            title.add(
                Text::new("BLIT / LAYOUT PLAYGROUND")
                    .color(colors::TEXT)
                    .text_size(23.0),
            );
            title.add(
                Text::new("Configure one layout, then resize its container to watch flex react")
                    .color(colors::TEXT_MUTED)
                    .text_size(12.0),
            );
        });
        if header
            .add(
                choice("Reset", false)
                    .id("reset playground")
                    .padding_x(14.0),
            )
            .clicked()
        {
            self.canvas = CanvasConfig::default();
            self.transition_easing = Easing::EaseInOutQuad;
            self.transition_target = false;
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
            .layout(
                Flex::column()
                    .padding(LogicalInsets::uniform(15.0))
                    .gap(11.0),
            )
            .width(Sizing::fixed(310.0))
            .height(Sizing::grow())
            .background(colors::SURFACE)
            .border(1.0, colors::BORDER)
            .uniform_radius(11.0)
            .clip(Clip::Bounds)
            .open();
        controls.add(
            Text::new("LAYOUT PARAMETERS")
                .color(colors::ACCENT)
                .text_size(12.0),
        );

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
            Text::new("The ten numbered rectangles share these parameters. Grow consumes free space; Fit uses the label's intrinsic size.")
                .color(colors::TEXT_DIM)
                .text_size(11.0)
                .wrap(TextWrap::Word),
        );
    }

    fn preview(&mut self, ui: &mut Ui, max_width: f32, max_height: f32) {
        let mut preview = ui
            .layout(
                Flex::column()
                    .padding(LogicalInsets::uniform(12.0))
                    .gap(9.0),
            )
            .grow()
            .background(colors::SURFACE)
            .border(1.0, colors::BORDER)
            .uniform_radius(11.0)
            .open();
        preview.add(
            Text::new("DRAG THE RIGHT EDGE, BOTTOM EDGE, OR CORNER • ZOOM SCALES CONTENT")
                .color(colors::TEXT_DIM)
                .text_size(10.0),
        );

        preview.add(|ui: &mut Ui| self.playground(ui, max_width, max_height));
        preview.add(|ui: &mut Ui| self.transitions(ui));
    }

    fn playground(&mut self, ui: &mut Ui, max_width: f32, max_height: f32) {
        let mut viewport = ui
            .layout(Flex::column().padding(LogicalInsets::uniform(8.0)))
            .grow()
            .background(colors::TRACK)
            .uniform_radius(8.0)
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
            .layout(
                Flex::column()
                    .padding(LogicalInsets::uniform(10.0))
                    .gap(8.0),
            )
            .width(Sizing::grow())
            .height(Sizing::fixed(156.0))
            .background(colors::TRACK)
            .border(1.0, colors::BORDER)
            .uniform_radius(8.0)
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
                        choice(label, self.transition_easing == easing)
                            .id(("transition easing", index))
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
                choice("Reverse", self.transition_target)
                    .id("reverse transitions")
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
                .layout(Flex::row().padding(LogicalInsets::uniform(7.0)))
                .width(Sizing::percent(0.5))
                .height(Sizing::grow())
                .background(colors::SURFACE)
                .uniform_radius(6.0)
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
                    .background(colors::SURFACE_BLUE)
                    .uniform_radius(5.0)
                    .open();
                specimen.add(Text::new("X / Y").color(colors::WHITE).text_size(10.0));
            });
        });
        tracks.add(|ui: &mut Ui| {
            let mut track = ui
                .layout(Flex::row().padding(LogicalInsets::uniform(7.0)))
                .width(Sizing::percent(0.5))
                .height(Sizing::grow())
                .background(colors::SURFACE)
                .uniform_radius(6.0)
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
                    .background(colors::SURFACE_PURPLE)
                    .uniform_radius(5.0)
                    .open();
                specimen.add(Text::new("W / H").color(colors::WHITE).text_size(10.0));
            });
        });
    }
    fn screen_badge(ui: &mut Ui) {
        let mut screen_badge = ui
            .layout(
                Flex::row()
                    .padding(LogicalInsets {
                        top: 6.0,
                        right: 10.0,
                        bottom: 6.0,
                        left: 10.0,
                    })
                    .gap(6.0)
                    .align(Align::Center),
            )
            .background(colors::SURFACE_HIGH)
            .border(1.0, colors::ACCENT)
            .uniform_radius(7.0)
            .z_index(20)
            .absolute(
                Absolute::screen(0.0, 0.0)
                    .anchors(Anchor::BottomRight, Anchor::BottomRight)
                    .offset(-16.0, -16.0),
            )
            .open();
        screen_badge.add(
            Rectangle::new()
                .fixed(6.0, 6.0)
                .background(colors::ACCENT)
                .uniform_radius(3.0),
        );
        screen_badge.add(
            Text::new("SCREEN ABSOLUTE")
                .color(colors::TEXT)
                .text_size(10.0),
        );
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
                .fixed(marker_width, marker_height)
                .background(if self.interaction.hovered || self.interaction.dragged {
                    colors::ACCENT
                } else {
                    color
                })
                .uniform_radius(marker_width.min(marker_height) / 2.0),
        );
        if matches!(self.edge, ResizeEdge::Right)
            && (self.interaction.hovered || self.interaction.dragged)
        {
            grip.add(|ui: &mut Ui| {
                let mut readout = ui
                    .layout(Flex::row().padding(LogicalInsets {
                        top: 4.0,
                        right: 7.0,
                        bottom: 4.0,
                        left: 7.0,
                    }))
                    .background(colors::BACKGROUND)
                    .border(1.0, colors::ACCENT)
                    .uniform_radius(5.0)
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
        let mut layout = ui
            .layout(
                Flex::new(config.axis)
                    .padding(LogicalInsets::uniform(config.padding * config.zoom))
                    .gap(config.gap * config.zoom)
                    .align(config.align)
                    .justify(config.justify),
            )
            .grow()
            .background(colors::CANVAS)
            .border(2.0, colors::CANVAS_BORDER)
            .uniform_radius(8.0)
            .clip(Clip::Bounds)
            .open();

        for (index, label) in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]
            .into_iter()
            .enumerate()
        {
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
            layout.add(|ui: &mut Ui| {
                let item = ui.layout(
                    Flex::column()
                        .align(Align::Stretch)
                        .justify(Justify::Center),
                );
                let item = match config.axis {
                    Axis::Horizontal => item.width(main).height(cross),
                    Axis::Vertical => item.width(cross).height(main),
                };
                let mut rectangle = item
                    .background(colors::ITEMS[index])
                    .uniform_radius(5.0)
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
                            .background(colors::BACKGROUND)
                            .border(1.0, colors::WHITE)
                            .uniform_radius(5.0)
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
            });
        }
    }
}

struct Button<'a> {
    label: &'a str,
    id: WidgetId,
    background: blit::color::Color,
    clicked_background: blit::color::Color,
    text_color: blit::color::Color,
    border_width: f32,
    border_color: blit::color::Color,
    radius: f32,
    padding_x: f32,
    padding_y: f32,
    text_size: f32,
}

struct ButtonResponse(bool);

impl<'a> Button<'a> {
    fn new(label: &'a str) -> Self {
        Self {
            label,
            id: WidgetId::new(label),
            background: colors::SURFACE_HIGH,
            clicked_background: colors::ACCENT,
            text_color: colors::TEXT,
            border_width: 0.0,
            border_color: colors::BORDER,
            radius: 0.0,
            padding_x: 8.0,
            padding_y: 8.0,
            text_size: 12.0,
        }
    }

    fn id(mut self, source: impl std::hash::Hash) -> Self {
        self.id = WidgetId::new(source);
        self
    }

    fn background(mut self, color: blit::color::Color) -> Self {
        self.background = color;
        self
    }

    fn clicked_background(mut self, color: blit::color::Color) -> Self {
        self.clicked_background = color;
        self
    }

    fn text_color(mut self, color: blit::color::Color) -> Self {
        self.text_color = color;
        self
    }

    fn border(mut self, width: f32, color: blit::color::Color) -> Self {
        self.border_width = width;
        self.border_color = color;
        self
    }

    fn uniform_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    fn padding_x(mut self, padding: f32) -> Self {
        self.padding_x = padding;
        self
    }

    fn padding_y(mut self, padding: f32) -> Self {
        self.padding_y = padding;
        self
    }

    fn text_size(mut self, size: f32) -> Self {
        self.text_size = size;
        self
    }
}

impl ButtonResponse {
    fn clicked(self) -> bool {
        self.0
    }
}

impl Widget for Button<'_> {
    type Output = ButtonResponse;

    fn render(self, ui: &mut Ui) -> ButtonResponse {
        let interaction = ui.interact(self.id, Sense::CLICK);
        let mut button = ui
            .layout(Flex::row().padding(LogicalInsets {
                top: self.padding_y,
                right: self.padding_x,
                bottom: self.padding_y,
                left: self.padding_x,
            }))
            .id(self.id)
            .style(
                Style::new()
                    .background(if interaction.pressed || interaction.clicked {
                        self.clicked_background
                    } else {
                        self.background
                    })
                    .solid_border(self.border_width, self.border_color),
            )
            .uniform_radius(self.radius)
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
                .add(
                    choice(label, *selected == value)
                        .id((id, index))
                        .padding_x(padding_x),
                )
                .clicked()
            {
                *selected = value;
            }
        }
    }
}

fn choice(label: &str, selected: bool) -> Button<'_> {
    Button::new(label)
        .background(if selected {
            colors::ACCENT_DARK
        } else {
            colors::SURFACE_HIGH
        })
        .clicked_background(colors::ACCENT)
        .text_color(colors::TEXT)
        .border(
            1.0,
            if selected {
                colors::ACCENT
            } else {
                colors::BORDER
            },
        )
        .uniform_radius(6.0)
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
