use blit::KeyboardEvent;
use blit_tui::{
    Ui,
    atom::Border,
    color::Color,
    interact,
    layout::{Justify, flex},
    text::{HorizontalAlign, TextAttributes, TextOptions},
    widget::{Block, Text},
};

fn main() -> std::io::Result<()> {
    blit_tui::run(|ui| {
        let mut root = ui.layout(flex::column().gap(1).pad(1));

        let quit = root.child().build(|ui: Ui<'_>| {
            let mut header = ui.layout(flex::row().justify(Justify::SpaceBetween).pad(1));

            header.insert(Block::new().border(Border::new(Color::BLUE)));
            header.child().build(|mut ui: Ui<'_>| {
                ui.insert(Text::new("hello from blit!").attributes(TextAttributes::BOLD));
            });

            header.child().item(flex::item().fixed(8, 1)).build(|mut ui: Ui<'_>| {
                let press = ui.interact(interact::press());

                ui.insert(Block::new().background(Color::BLUE));
                ui.insert(Text::new("quit").options(TextOptions::new().horizontal_align(HorizontalAlign::Center)));

                press.clicked
            })
        });
        let quit_key = root.interact(interact::keyboard(|input| match input {
            KeyboardEvent::Text('q') => Some(()),
            _ => None,
        }));
        if quit || quit_key.is_some() {
            root.context_mut().quit();
        }
    })
}
