use blit::{
    Align, Axis, Clip, Container, Justify, Sizing, Ui,
    geometry::LogicalInsets,
    interact::{Sense, WidgetId},
    paint::{FontId, HorizontalAlign, TextWrap},
    platform::Platform,
    widget::{Button, Rectangle, Text},
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

    fn new(_: Platform, _: EventLoopProxy<Self::Input>, _: Root<Self>) -> Self {
        Self {
            axis: 0,
            justify: 0,
            align: 1,
            sizing: 0,
            zoom: 2,
            gap: 2,
            padding: 2,
            width: 640.0,
            height: 440.0,
        }
    }

    fn input(&mut self, _: Self::Input) {}

    fn render(&mut self, ui: &mut Ui) {
        ui.clear();
        let screen = ui.screen();
        let max_width = (screen.width - 420.0).max(240.0);
        let max_height = (screen.height - 190.0).max(180.0);
        self.width = self.width.clamp(240.0, max_width);
        self.height = self.height.clamp(180.0, max_height);

        let mut root = ui.column(
            Container::new()
                .grow()
                .padding(LogicalInsets::uniform(20.0))
                .gap(16.0)
                .background(colors::BACKGROUND),
        );

        {
            let mut header = root.row(
                Container::new()
                    .width(Sizing::grow())
                    .align(Align::Center)
                    .justify(Justify::SpaceBetween)
                    .background(colors::BACKGROUND),
            );
            {
                let mut title = header.column(Container::new().gap(3.0));
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
            }
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
                self.width = 640.0;
                self.height = 440.0;
            }
        }

        {
            let mut body = root.row(Container::new().grow().gap(16.0).clip(Clip::Bounds));

            {
                let mut controls = body.column(
                    Container::new()
                        .width(Sizing::fixed(310.0))
                        .height(Sizing::grow())
                        .padding(LogicalInsets::uniform(15.0))
                        .gap(11.0)
                        .background(colors::SURFACE)
                        .border(1.0, colors::BORDER)
                        .uniform_radius(11.0)
                        .clip(Clip::Bounds),
                );
                controls.add(
                    Text::new("LAYOUT PARAMETERS")
                        .color(colors::ACCENT)
                        .text_size(12.0),
                );

                controls.add(Text::new("axis").color(colors::TEXT_MUTED).text_size(11.0));
                {
                    let mut row = controls.row(Container::new().width(Sizing::grow()).gap(6.0));
                    for (index, label) in ["Horizontal", "Vertical"].into_iter().enumerate() {
                        if row
                            .add(choice(label, self.axis == index).id(("axis", index)))
                            .clicked()
                        {
                            self.axis = index;
                        }
                    }
                }

                controls.add(
                    Text::new("justify")
                        .color(colors::TEXT_MUTED)
                        .text_size(11.0),
                );
                for start in [0, 3] {
                    let mut row = controls.row(Container::new().width(Sizing::grow()).gap(6.0));
                    for (index, label) in ["Start", "Center", "End", "Between", "Around", "Evenly"]
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
                }

                controls.add(Text::new("align").color(colors::TEXT_MUTED).text_size(11.0));
                {
                    let mut row = controls.row(Container::new().width(Sizing::grow()).gap(5.0));
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
                }

                controls.add(
                    Text::new("item sizing")
                        .color(colors::TEXT_MUTED)
                        .text_size(11.0),
                );
                {
                    let mut row = controls.row(Container::new().width(Sizing::grow()).gap(6.0));
                    for (index, label) in ["Fixed", "Fit", "Grow"].into_iter().enumerate() {
                        if row
                            .add(choice(label, self.sizing == index).id(("sizing", index)))
                            .clicked()
                        {
                            self.sizing = index;
                        }
                    }
                }

                controls.add(Text::new("zoom").color(colors::TEXT_MUTED).text_size(11.0));
                {
                    let mut row = controls.row(Container::new().width(Sizing::grow()).gap(5.0));
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
                }

                controls.add(Text::new("gap").color(colors::TEXT_MUTED).text_size(11.0));
                {
                    let mut row = controls.row(Container::new().width(Sizing::grow()).gap(5.0));
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
                }

                controls.add(
                    Text::new("padding")
                        .color(colors::TEXT_MUTED)
                        .text_size(11.0),
                );
                {
                    let mut row = controls.row(Container::new().width(Sizing::grow()).gap(5.0));
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
                }

                controls.add(
                    Text::new("The ten numbered rectangles share these parameters. Grow consumes free space; Fit uses the label's intrinsic size.")
                        .color(colors::TEXT_DIM)
                        .text_size(11.0)
                        .wrap(TextWrap::Word),
                );
            }

            {
                let mut preview = body.column(
                    Container::new()
                        .grow()
                        .padding(LogicalInsets::uniform(12.0))
                        .gap(9.0)
                        .background(colors::SURFACE)
                        .border(1.0, colors::BORDER)
                        .uniform_radius(11.0),
                );
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

                let mut viewport = preview.column(
                    Container::new()
                        .grow()
                        .padding(LogicalInsets::uniform(8.0))
                        .background(colors::TRACK)
                        .uniform_radius(8.0)
                        .clip(Clip::Bounds),
                );
                let mut shell =
                    viewport.column(Container::new().fixed(self.width + 12.0, self.height + 12.0));

                let width_delta = {
                    let mut row = shell.row(Container::new().fixed(self.width + 12.0, self.height));
                    {
                        let mut layout = row.flow(
                            axis,
                            Container::new()
                                .fixed(self.width, self.height)
                                .padding(LogicalInsets::uniform(padding))
                                .gap(gap)
                                .align(align)
                                .justify(justify)
                                .background(colors::CANVAS)
                                .border(2.0, colors::CANVAS_BORDER)
                                .uniform_radius(8.0)
                                .clip(Clip::Bounds),
                        );

                        for (index, label) in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]
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
                            let item = match axis {
                                Axis::Horizontal => Container::new().width(main).height(cross),
                                Axis::Vertical => Container::new().width(cross).height(main),
                            };
                            let mut rectangle = layout.column(
                                item.align(Align::Stretch)
                                    .justify(Justify::Center)
                                    .background(colors::ITEMS[index])
                                    .uniform_radius(5.0),
                            );
                            rectangle.add(
                                Text::new(label)
                                    .color(colors::WHITE)
                                    .text_size(11.0 * zoom)
                                    .align(HorizontalAlign::Center),
                            );
                        }
                    }
                    {
                        let id = WidgetId::new("layout width grip");
                        let mut grip = row.column(
                            Container::new()
                                .fixed(12.0, self.height)
                                .align(Align::Center)
                                .justify(Justify::Center)
                                .id(id)
                                .interact(id, Sense::DRAG),
                        );
                        let interaction = grip.interaction();
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
                        interaction.drag_delta.x
                    }
                };

                let (height_delta, corner_delta) = {
                    let mut row = shell.row(Container::new().fixed(self.width + 12.0, 12.0));
                    let height_delta = {
                        let id = WidgetId::new("layout height grip");
                        let mut grip = row.row(
                            Container::new()
                                .fixed(self.width, 12.0)
                                .align(Align::Center)
                                .justify(Justify::Center)
                                .id(id)
                                .interact(id, Sense::DRAG),
                        );
                        let interaction = grip.interaction();
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
                    };
                    let corner_delta = {
                        let id = WidgetId::new("layout corner grip");
                        let mut grip = row.row(
                            Container::new()
                                .fixed(12.0, 12.0)
                                .align(Align::Center)
                                .justify(Justify::Center)
                                .id(id)
                                .interact(id, Sense::DRAG),
                        );
                        let interaction = grip.interaction();
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
                    };
                    (height_delta, corner_delta)
                };

                self.width = (self.width + width_delta + corner_delta.x).clamp(240.0, max_width);
                self.height =
                    (self.height + height_delta + corner_delta.y).clamp(180.0, max_height);
            }
        }
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
    width: f32,
    height: f32,
}

fn choice(label: &'static str, selected: bool) -> Button {
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
