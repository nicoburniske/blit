use blit::{Input, Sense, Sides, WidgetId};
use blit_tui::{
    Ui,
    atom::Border,
    color::Color,
    layout::{Justify, flex},
    text::{HorizontalAlign, TextAttributes, TextOptions},
    widget::{Block, Text},
};

fn main() -> std::io::Result<()> {
    blit_tui::run(|ui| {
        let mut root = ui.layout(flex::column().padding(Sides::all(1.0)).gap(1.0));

        let quit = root.child(flex::item()).build(|ui: Ui<'_>| {
            let mut header = ui.layout(
                flex::row()
                    .padding(Sides::all(1.0))
                    .justify(Justify::SpaceBetween),
            );

            header.insert(Block::new().border(Border::new(Color::BLUE)));
            header.child(flex::item()).build(|mut ui: Ui<'_>| {
                ui.insert(Text::new("hello from blit!").attributes(TextAttributes::BOLD));
            });

            header
                .child(flex::item().fixed(8.0, 1.0))
                .build(|mut ui: Ui<'_>| {
                    let id = WidgetId::new("quit");
                    let interaction = ui.interact(id, Sense::CLICK);

                    let mut button = ui.widget_id(id);
                    button.insert(Block::new().background(Color::BLUE));
                    button.insert(
                        Text::new("quit")
                            .options(TextOptions::new().horizontal_align(HorizontalAlign::Center)),
                    );

                    interaction.clicked
                })
        });
        if quit || matches!(root.input(), Input::Text('q')) {
            root.platform().quit();
        }
    })
}
