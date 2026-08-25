use std::time::Duration;

use blit::{
    Ui,
    animation::{Easing, Transition},
    container::{Absolute, Anchor, Sizing},
    geometry::LogicalInsets,
    input::{Input, Key},
    interact::{Sense, WidgetId},
    layout::{Align, Axis, Flex, Justify},
    style::{Clip, Style},
    text::{FontId, HorizontalAlign, TextWrap},
    widget::{Rectangle, Text, Widget},
};
use blit_cpu::{Font, FontFace, RendererConfig};
use blit_desktop::{Application, Config, EventLoopProxy, Root};

fn main() {
    blit_desktop::run::<Showcase>(Config {
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

impl Application for Showcase {
    type Input = ();

    fn new(_: EventLoopProxy<Self::Input>, _: Root<Self>) -> Self {
        Self {
            axis: 0,
            justify: 0,
            align: 1,
            sizing: 0,
            zoom: 2,
            gap: 2,
            padding: 2,
            transition_easing: 3,
            transition_target: false,
            width: 640.0,
            height: 440.0,
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
        self.width = self.width.clamp(240.0, max_width);
        self.height = self.height.clamp(180.0, max_height);

        let mut root = ui
            .layout(
                Flex::column()
                    .padding(LogicalInsets::uniform(20.0))
                    .gap(16.0),
            )
            .grow()
            .background(colors::BACKGROUND)
            .open();

        root.add(|ui: &mut Ui| {
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
                    Text::new(
                        "Configure one layout, then resize its container to watch flex react",
                    )
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
                self.axis = 0;
                self.justify = 0;
                self.align = 1;
                self.sizing = 0;
                self.zoom = 2;
                self.gap = 2;
                self.padding = 2;
                self.transition_easing = 3;
                self.transition_target = false;
                self.width = 640.0;
                self.height = 440.0;
            }
        });

        root.add(|ui: &mut Ui| {
            let mut body = ui
                .layout(Flex::row().gap(16.0))
                .grow()
                .clip(Clip::Bounds)
                .open();

            body.add(|ui: &mut Ui| {
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
                controls.add(|ui: &mut Ui| {
                    let mut row = ui
                        .layout(Flex::row().gap(6.0))
                        .width(Sizing::grow())
                        .open();
                    for (index, label) in ["Horizontal", "Vertical"].into_iter().enumerate() {
                        if row
                            .add(choice(label, self.axis == index).id(("axis", index)))
                            .clicked()
                        {
                            self.axis = index;
                        }
                    }
                });

                controls.add(
                    Text::new("justify")
                        .color(colors::TEXT_MUTED)
                        .text_size(11.0),
                );
                for start in [0, 3] {
                    controls.add(|ui: &mut Ui| {
                        let mut row = ui
                            .layout(Flex::row().gap(6.0))
                            .width(Sizing::grow())
                            .open();
                        for (index, label) in
                            ["Start", "Center", "End", "Between", "Around", "Evenly"]
                                .into_iter()
                                .enumerate()
                                .skip(start)
                                .take(3)
                        {
                            if row
                                .add(
                                    choice(label, self.justify == index)
                                        .id(("justify", index))
                                        .padding_x(7.0),
                                )
                                .clicked()
                            {
                                self.justify = index;
                            }
                        }
                    });
                }

                controls.add(Text::new("align").color(colors::TEXT_MUTED).text_size(11.0));
                controls.add(|ui: &mut Ui| {
                    let mut row = ui
                        .layout(Flex::row().gap(5.0))
                        .width(Sizing::grow())
                        .open();
                    for (index, label) in ["Start", "Center", "End", "Stretch"]
                        .into_iter()
                        .enumerate()
                    {
                        if row
                            .add(
                                choice(label, self.align == index)
                                    .id(("align", index))
                                    .padding_x(6.0),
                            )
                            .clicked()
                        {
                            self.align = index;
                        }
                    }
                });

                controls.add(
                    Text::new("item sizing")
                        .color(colors::TEXT_MUTED)
                        .text_size(11.0),
                );
                controls.add(|ui: &mut Ui| {
                    let mut row = ui
                        .layout(Flex::row().gap(6.0))
                        .width(Sizing::grow())
                        .open();
                    for (index, label) in ["Fixed", "Fit", "Grow"].into_iter().enumerate() {
                        if row
                            .add(choice(label, self.sizing == index).id(("sizing", index)))
                            .clicked()
                        {
                            self.sizing = index;
                        }
                    }
                });

                controls.add(Text::new("zoom").color(colors::TEXT_MUTED).text_size(11.0));
                controls.add(|ui: &mut Ui| {
                    let mut row = ui
                        .layout(Flex::row().gap(5.0))
                        .width(Sizing::grow())
                        .open();
                    for (index, label) in ["50%", "75%", "100%", "125%", "150%"]
                        .into_iter()
                        .enumerate()
                    {
                        if row
                            .add(
                                choice(label, self.zoom == index)
                                    .id(("zoom", index))
                                    .padding_x(5.0),
                            )
                            .clicked()
                        {
                            self.zoom = index;
                        }
                    }
                });

                controls.add(Text::new("gap").color(colors::TEXT_MUTED).text_size(11.0));
                controls.add(|ui: &mut Ui| {
                    let mut row = ui
                        .layout(Flex::row().gap(5.0))
                        .width(Sizing::grow())
                        .open();
                    for (index, label) in ["0", "4", "8", "16", "24"].into_iter().enumerate() {
                        if row
                            .add(
                                choice(label, self.gap == index)
                                    .id(("gap", index))
                                    .padding_x(9.0),
                            )
                            .clicked()
                        {
                            self.gap = index;
                        }
                    }
                });

                controls.add(
                    Text::new("padding")
                        .color(colors::TEXT_MUTED)
                        .text_size(11.0),
                );
                controls.add(|ui: &mut Ui| {
                    let mut row = ui
                        .layout(Flex::row().gap(5.0))
                        .width(Sizing::grow())
                        .open();
                    for (index, label) in ["0", "4", "8", "16", "24"].into_iter().enumerate() {
                        if row
                            .add(
                                choice(label, self.padding == index)
                                    .id(("padding", index))
                                    .padding_x(9.0),
                            )
                            .clicked()
                        {
                            self.padding = index;
                        }
                    }
                });

                controls.add(
                    Text::new("The ten numbered rectangles share these parameters. Grow consumes free space; Fit uses the label's intrinsic size.")
                        .color(colors::TEXT_DIM)
                        .text_size(11.0)
                        .wrap(TextWrap::Word),
                );
            });

            body.add(|ui: &mut Ui| {
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

                let zoom = [0.5, 0.75, 1.0, 1.25, 1.5][self.zoom];
                let gap = [0.0, 4.0, 8.0, 16.0, 24.0][self.gap] * zoom;
                let padding = [0.0, 4.0, 8.0, 16.0, 24.0][self.padding] * zoom;
                let axis = [Axis::Horizontal, Axis::Vertical][self.axis];
                let justify = [
                    Justify::Start,
                    Justify::Center,
                    Justify::End,
                    Justify::SpaceBetween,
                    Justify::SpaceAround,
                    Justify::SpaceEvenly,
                ][self.justify];
                let align = [Align::Start, Align::Center, Align::End, Align::Stretch][self.align];

                preview.add(|ui: &mut Ui| {
                    let mut viewport = ui
                        .layout(Flex::column().padding(LogicalInsets::uniform(8.0)))
                        .grow()
                        .background(colors::TRACK)
                        .uniform_radius(8.0)
                        .clip(Clip::Bounds)
                        .open();
                    let (width_delta, height_delta, corner_delta) =
                        viewport.add(|ui: &mut Ui| {
                            let mut shell = ui
                                .layout(Flex::column())
                                .fixed(self.width + 12.0, self.height + 12.0)
                                .open();

                            let width_delta = shell.add(|ui: &mut Ui| {
                                let mut row = ui
                                    .layout(Flex::row())
                                    .fixed(self.width + 12.0, self.height)
                                    .open();
                                row.add(|ui: &mut Ui| {
                                    let mut layout = ui
                                        .layout(
                                    Flex::new(axis)
                                        .padding(LogicalInsets::uniform(padding))
                                        .gap(gap)
                                        .align(align)
                                        .justify(justify),
                                )
                                .fixed(self.width, self.height)
                                .background(colors::CANVAS)
                                .border(2.0, colors::CANVAS_BORDER)
                                .uniform_radius(8.0)
                                .clip(Clip::Bounds)
                                .open();

                            for (index, label) in
                                ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]
                                    .into_iter()
                                    .enumerate()
                            {
                                let main = (26.0 + (index % 5) as f32 * 5.0) * zoom;
                                let cross = (48.0 + (index % 4) as f32 * 13.0) * zoom;
                                let main = match self.sizing {
                                    0 => Sizing::fixed(main),
                                    1 => Sizing::fit().min(20.0 * zoom).max(main),
                                    _ => Sizing::grow().min(20.0 * zoom),
                                };
                                let cross = if align == Align::Stretch {
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
                                    let item = match axis {
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
                                            .text_size(11.0 * zoom)
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
                                                .layout(
                                                    Flex::row()
                                                        .align(Align::Center)
                                                        .justify(Justify::Center),
                                                )
                                                .fixed(
                                                    (28.0 * zoom).max(20.0),
                                                    (14.0 * zoom).max(10.0),
                                                )
                                                .background(colors::BACKGROUND)
                                                .border(1.0, colors::WHITE)
                                                .uniform_radius(5.0)
                                                .absolute(
                                                    Absolute::attach(anchor, Anchor::Center)
                                                        .z_index(1),
                                                )
                                                .open();
                                            badge.add(
                                                Text::new("ABS")
                                                    .color(colors::WHITE)
                                                    .text_size((8.0 * zoom).max(7.0)),
                                            );
                                        });
                                    }
                                });
                            }
                                });
                                row.add(|ui: &mut Ui| {
                            let id = WidgetId::new("layout width grip");
                            let interaction = ui.interact(id, Sense::DRAG);
                            let mut grip = ui
                                .layout(
                                    Flex::column().align(Align::Center).justify(Justify::Center),
                                )
                                .fixed(12.0, self.height)
                                .id(id)
                                .open();
                            grip.add(
                                Rectangle::new()
                                    .fixed(3.0, 48.0)
                                    .background(if interaction.hovered || interaction.dragged {
                                        colors::ACCENT
                                    } else {
                                        colors::GRIP
                                    })
                                    .uniform_radius(1.5),
                            );
                            if interaction.hovered || interaction.dragged {
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
                                        .absolute(
                                            Absolute::attach(Anchor::Left, Anchor::Right)
                                                .offset(-8.0, 0.0)
                                                .z_index(10),
                                        )
                                        .open();
                                    readout.add(
                                        Text::new("DRAG X").color(colors::TEXT).text_size(9.0),
                                    );
                                });
                            }
                            interaction.drag_delta.x
                                })
                            });

                    let (height_delta, corner_delta) = shell.add(|ui: &mut Ui| {
                        let mut row = ui
                            .layout(Flex::row())
                            .fixed(self.width + 12.0, 12.0)
                            .open();
                        let height_delta = row.add(|ui: &mut Ui| {
                            let id = WidgetId::new("layout height grip");
                            let interaction = ui.interact(id, Sense::DRAG);
                            let mut grip = ui
                                .layout(Flex::row().align(Align::Center).justify(Justify::Center))
                                .fixed(self.width, 12.0)
                                .id(id)
                                .open();
                            grip.add(
                                Rectangle::new()
                                    .fixed(48.0, 3.0)
                                    .background(if interaction.hovered || interaction.dragged {
                                        colors::ACCENT
                                    } else {
                                        colors::GRIP
                                    })
                                    .uniform_radius(1.5),
                            );
                            interaction.drag_delta.y
                        });
                        let corner_delta = row.add(|ui: &mut Ui| {
                            let id = WidgetId::new("layout corner grip");
                            let interaction = ui.interact(id, Sense::DRAG);
                            let mut grip = ui
                                .layout(Flex::row().align(Align::Center).justify(Justify::Center))
                                .fixed(12.0, 12.0)
                                .id(id)
                                .open();
                            grip.add(
                                Rectangle::new()
                                    .fixed(6.0, 6.0)
                                    .background(if interaction.hovered || interaction.dragged {
                                        colors::ACCENT
                                    } else {
                                        colors::GRIP_CORNER
                                    })
                                    .uniform_radius(3.0),
                            );
                            interaction.drag_delta
                        });
                        (height_delta, corner_delta)
                    });
                            (width_delta, height_delta, corner_delta)
                        });

                    self.width =
                        (self.width + width_delta + corner_delta.x).clamp(240.0, max_width);
                    self.height =
                        (self.height + height_delta + corner_delta.y).clamp(180.0, max_height);
                });

                preview.add(|ui: &mut Ui| {
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
                transitions.add(|ui: &mut Ui| {
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
                        for (index, label) in
                            ["Linear", "In", "Out", "In-out"].into_iter().enumerate()
                        {
                            if choices
                                .add(
                                    choice(label, self.transition_easing == index)
                                        .id(("transition easing", index))
                                        .padding_x(7.0)
                                        .padding_y(4.0),
                                )
                                .clicked()
                            {
                                self.transition_easing = index;
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
                });

                let easing = [
                    Easing::Linear,
                    Easing::EaseInQuad,
                    Easing::EaseOutQuad,
                    Easing::EaseInOutQuad,
                ][self.transition_easing];
                transitions.add(|ui: &mut Ui| {
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
                });
                });
            });
        });

        root.add(|ui: &mut Ui| {
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
                .absolute(
                    Absolute::screen(0.0, 0.0)
                        .anchors(Anchor::BottomRight, Anchor::BottomRight)
                        .offset(-16.0, -16.0)
                        .z_index(20),
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
        });
    }
}

struct Showcase {
    axis: usize,
    justify: usize,
    align: usize,
    sizing: usize,
    zoom: usize,
    gap: usize,
    padding: usize,
    transition_easing: usize,
    transition_target: bool,
    width: f32,
    height: f32,
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

impl Button<'static> {
    fn new(label: &'static str) -> Self {
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

fn choice(label: &'static str, selected: bool) -> Button<'static> {
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
