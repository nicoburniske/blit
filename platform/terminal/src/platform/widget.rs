use blit::Widget;
use blit_term::{color::Color, text::TextOptions};

use super::{TerminalPlatform, draw::TextRun};
use crate::Ui;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Text<'a> {
        new(text: &'a str),
        color: Color = Color::WHITE,
        bold: bool = false,
        options: TextOptions = TextOptions::new(),
    }
}

impl Widget<TerminalPlatform> for Text<'_> {
    type Response = ();

    fn build(self, ui: &mut Ui<'_>) {
        let run = ui.platform().text_run(self.text);
        ui.add(
            TextRun::new(run)
                .color(self.color)
                .bold(self.bold)
                .options(self.options),
        );
    }
}
