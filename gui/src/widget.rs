use blit::{Widget, state};

use crate::{
    atom,
    text::{Span, TextOptions, TextSource, TextStyle},
};

blit::builder! {
    #[derive(Clone, Copy, Debug)]
    pub struct Text<'a> {
        new(#[into] text: TextSource<'a>),
        style: TextStyle = TextStyle::new(),
        offset_x: f32 = 0.0,
        options: TextOptions = TextOptions::new(),
    }
}

impl<C: crate::Context> Widget<C, state::Node> for Text<'_> {
    type Response = ();

    fn build(self, mut ui: blit::Ui<'_, C, state::Node>) {
        let run = ui.context_mut().as_mut().text_run(self.text, self.style);
        let colors = ui.store_slice(std::iter::once(self.style.color));
        ui.insert(
            atom::Text::new(run, colors)
                .offset_x(self.offset_x)
                .options(self.options),
        );
    }
}

blit::builder! {
    /// text partitioned by ordered, contiguous spans
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct RichText<'a> {
        new(text: &'a str, spans: &'a [Span]),
        style: TextStyle = TextStyle::new(),
        offset_x: f32 = 0.0,
        options: TextOptions = TextOptions::new(),
    }
}

impl<C: crate::Context> Widget<C, state::Node> for RichText<'_> {
    type Response = ();

    fn build(self, mut ui: blit::Ui<'_, C, state::Node>) {
        let run = ui.context_mut().as_mut().rich_text(self.text, self.spans, self.style);
        // empty spans lay out as one span in the base color
        let colors = if self.spans.is_empty() {
            ui.store_slice(std::iter::once(self.style.color))
        } else {
            ui.store_slice(
                self.spans
                    .iter()
                    .map(|span| span.style.color.unwrap_or(self.style.color)),
            )
        };
        ui.insert(
            atom::Text::new(run, colors)
                .offset_x(self.offset_x)
                .options(self.options),
        );
    }
}
