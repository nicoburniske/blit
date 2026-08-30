use std::io;

use blit::{Input, Key, Sense, Sides, Sizing, Slot, Ui, WidgetId};
use blit_showcase::Showcase;
use blit_terminal::{
    ControlFlow, TerminalPlatform,
    color::Color,
    draw::{Block, Text},
    layout::Flex,
    text::TextStyle,
};

fn main() -> io::Result<()> {
    let mut showcase = Showcase::default();
    blit_terminal::run(|ui: &mut Ui<'_, TerminalPlatform>| {
        let control = if matches!(ui.input(), Input::Text('q'))
            || matches!(ui.input(), Input::Key(key) if key.key == Key::Escape)
        {
            ControlFlow::Exit
        } else {
            ControlFlow::Continue
        };
        showcase.input(ui.input());
        let style = TextStyle::default();
        let title = ui.platform().text_run(showcase.title(), style);
        let body = ui.platform().text_run(showcase.body(), style);
        let button_label = ui.platform().text_run("toggle terminal state", style);
        let mut root = ui.layout_with(
            Block::new(Color::from_rgba8(0, 0, 0, 255)),
            Flex::column().padding(Sides::all(1.0)).gap(1.0),
        );
        root.add(Slot::new().height(Sizing::fixed(1.0)), (), |mut ui| {
            ui.add(Text::new(title, style));
        });
        root.add(Slot::new().height(Sizing::fixed(2.0)), (), |mut ui| {
            ui.add(Text::new(body, style).color(Color::from_rgba8(180, 180, 180, 255)));
        });
        root.add(Slot::new().fixed(24.0, 3.0), (), |mut ui| {
            let id = WidgetId::new("terminal showcase button");
            let interaction = ui.interact(id, Sense::CLICK);
            if interaction.clicked {
                showcase.click();
            }
            let background = if interaction.active || showcase.enabled() {
                Color::from_rgba8(30, 80, 180, 255)
            } else {
                Color::from_rgba8(40, 40, 40, 255)
            };
            let mut button = ui
                .layout_with(
                    Block::new(background).border(1.0, Color::WHITE),
                    Flex::row().padding(Sides::all(1.0)),
                )
                .id(id);
            button.add(Slot::new(), (), |mut ui| {
                ui.add(Text::new(button_label, style));
            });
        });
        control
    })
}
