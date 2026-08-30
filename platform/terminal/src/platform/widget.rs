use blit::Widget;
use blit_term::{
    color::Color,
    text::{TextAttributes, TextOptions},
};

use super::{TerminalPlatform, draw::TextRun};
use crate::Ui;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Text<'a> {
        new(text: &'a str),
        color: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
        options: TextOptions = TextOptions::new(),
    }
}

impl Widget<TerminalPlatform> for Text<'_> {
    type Response = ();

    fn build(self, ui: &mut Ui) {
        let run = ui.platform().text_run(self.text);
        ui.add(
            TextRun::new(run)
                .color(self.color)
                .attributes(self.attributes)
                .options(self.options),
        );
    }
}
