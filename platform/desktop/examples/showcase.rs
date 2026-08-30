use blit::{Sense, Sides, Sizing, Slot, Ui, WidgetId};
use blit_cpu::{Font, FontFace, RendererConfig};
use blit_desktop::{
    Application, Config, DesktopPlatform, EventLoopProxy, Root,
    draw::{Rectangle, TextRun},
    layout::Flex,
    style::BorderRadius,
    text::{FontId, TextStyle},
};
use blit_showcase::Showcase;

fn main() {
    blit_desktop::run::<App>(Config {
        title: "Blit showcase".into(),
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

struct App(Showcase);

impl Application for App {
    type Input = ();

    fn new(_: EventLoopProxy<Self::Input>, _: Root<Self>, _: &mut DesktopPlatform) -> Self {
        Self(Showcase::default())
    }

    fn input(&mut self, _: Self::Input) {}

    fn render(&mut self, ui: &mut Ui<'_, DesktopPlatform>) {
        self.0.input(ui.input());
        let title_style = TextStyle {
            size: 24.0,
            ..TextStyle::default()
        };
        let body_style = TextStyle::default();
        let title = ui.platform().text_run(self.0.title(), title_style);
        let body = ui.platform().text_run(self.0.body(), body_style);
        let button_label = ui.platform().text_run("toggle platform state", body_style);
        let mut root = ui.layout_with(
            Rectangle::new().background(blit_desktop::color::Color::from_rgba8(20, 24, 32, 255)),
            Flex::column().padding(Sides::all(24.0)).gap(16.0),
        );
        root.add(Slot::new().height(Sizing::fixed(40.0)), (), |mut ui| {
            ui.add(TextRun::new(title, title_style).color(blit_desktop::color::Color::WHITE));
        });
        root.add(Slot::new().height(Sizing::fixed(32.0)), (), |mut ui| {
            ui.add(
                TextRun::new(body, body_style)
                    .color(blit_desktop::color::Color::from_rgba8(190, 198, 215, 255)),
            );
        });
        root.add(Slot::new().fixed(180.0, 44.0), (), |mut ui| {
            let id = WidgetId::new("showcase button");
            let interaction = ui.interact(id, Sense::CLICK);
            if interaction.clicked {
                self.0.click();
            }
            let color = if interaction.active || self.0.enabled() {
                blit_desktop::color::Color::from_rgba8(70, 110, 220, 255)
            } else {
                blit_desktop::color::Color::from_rgba8(48, 57, 76, 255)
            };
            let mut button = ui
                .layout_with(
                    Rectangle::new()
                        .background(color)
                        .radius(BorderRadius::uniform(8.0)),
                    Flex::row().padding(Sides::all(12.0)),
                )
                .id(id);
            button.add(Slot::new(), (), |mut ui| {
                ui.add(
                    TextRun::new(button_label, body_style).color(blit_desktop::color::Color::WHITE),
                );
            });
        });
    }
}
