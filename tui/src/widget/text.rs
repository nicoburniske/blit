use crate::{
    color::Color,
    text::{Span, TextAttributes, TextOptions},
};
use blit::{Content, state};

use crate::{TuiContext, Ui, atom};

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

impl Content<TuiContext> for Text<'_> {
    type Response = ();

    fn append(self, mut ui: Ui<'_, state::Node>) {
        let run = if let Some(spans) = self.spans {
            ui.context().renderer_mut().rich_text(spans)
        } else {
            ui.context().renderer_mut().text_run(self.text)
        };
        ui.insert(
            atom::Text::new(run)
                .color(self.color)
                .attributes(self.attributes)
                .options(self.options),
        );
    }
}
