use blit::Widget;
use blit_tui_render::{
    color::Color,
    text::{Span, TextAttributes, TextOptions},
};

use crate::{Cx, TuiPlatform, atom};

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

impl Widget<TuiPlatform> for Text<'_> {
    type Response = ();

    fn build(self, mut cx: Cx<'_>) {
        let run = if let Some(spans) = self.spans {
            cx.platform().rich_text(spans)
        } else {
            cx.platform().text_run(self.text)
        };
        cx.atom(
            atom::Text::new(run)
                .color(self.color)
                .attributes(self.attributes)
                .options(self.options),
        );
    }
}
