use blit::KeyboardEvent;
use blit_tui::{
    Ui,
    color::Color,
    interact,
    layout::{Justify, Pad, flex},
    text::{HorizontalAlign, Text, TextAttributes, TextOptions},
    widgets::block::{Block, Border},
};

fn main() -> std::io::Result<()> {
    blit_tui::run(|ui| {
        let mut root = ui.layout(flex::column().gap(1));
        root.insert(Pad::new(1));

        let quit = root.child().build(|ui: Ui<'_>| {
            let mut header = ui.layout(flex::row().justify(Justify::SpaceBetween));
            header.insert(Block::new().border(Border::new(Color::BLUE)));
            header.insert(Pad::new(1));
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
            root.context_mut().should_quit = true;
        }
    })
}
