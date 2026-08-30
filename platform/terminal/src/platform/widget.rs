use blit::Widget;
use blit_term::{
    color::Color,
    text::{Span, TextAttributes, TextOptions},
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
        spans: Option<&'a [Span<'a>]> = None,
    }
}

impl<'a> Text<'a> {
    pub fn rich(spans: &'a [Span<'a>]) -> Self {
        Self {
            text: "",
            color: Color::Reset,
            attributes: TextAttributes::NONE,
            options: TextOptions::new(),
            spans: Some(spans),
        }
    }
}

impl Widget<TerminalPlatform> for Text<'_> {
    type Response = ();

    fn build(self, ui: &mut Ui) {
        let run = if let Some(spans) = self.spans {
            ui.platform().rich_text(spans)
        } else {
            ui.platform().text_run(self.text)
        };
        ui.add(
            TextRun::new(run)
                .color(self.color)
                .attributes(self.attributes)
                .options(self.options),
        );
    }
}
