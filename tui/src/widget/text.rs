use std::fmt;

use blit::{Widget, state};

use crate::{
    Context, atom,
    color::Color,
    text::{Span, TextAttributes, TextOptions},
};

blit::builder! {
    /// text from borrowed strings or formatting arguments
    #[derive(Clone, Copy, Debug)]
    pub struct Text<'a> {
        new(#[into] text: TextSource<'a>),
        color: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
        options: TextOptions = TextOptions::new(),
        #[into]
        spans: Option<&'a [Span<'a>]> = None,
    }
}

impl<'a> Text<'a> {
    pub fn rich(spans: &'a [Span<'a>]) -> Self {
        Self::new("").spans(spans)
    }
}

impl<C: Context> Widget<C, state::Node> for Text<'_> {
    type Response = ();

    fn build(self, mut ui: blit::Ui<'_, C, state::Node>) {
        let run = if let Some(spans) = self.spans {
            ui.context_mut().as_mut().renderer_mut().rich_text(spans)
        } else {
            let renderer = ui.context_mut().as_mut().renderer_mut();
            match self.text {
                TextSource::Borrowed(text) => renderer.text_run(text),
                TextSource::Formatted(arguments) => renderer.text_run_fmt(arguments),
            }
        };
        ui.insert(
            atom::Text::new(run)
                .color(self.color)
                .attributes(self.attributes)
                .options(self.options),
        );
    }
}

/// text from borrowed strings or formatting arguments
#[derive(Clone, Copy, Debug)]
pub enum TextSource<'a> {
    Borrowed(&'a str),
    Formatted(fmt::Arguments<'a>),
}

impl<'a, T: AsRef<str> + ?Sized> From<&'a T> for TextSource<'a> {
    fn from(text: &'a T) -> Self {
        Self::Borrowed(T::as_ref(text))
    }
}

impl<'a, T: AsRef<str> + ?Sized> From<&'a mut T> for TextSource<'a> {
    fn from(text: &'a mut T) -> Self {
        Self::Borrowed(T::as_ref(text))
    }
}

impl<'a> From<fmt::Arguments<'a>> for TextSource<'a> {
    fn from(arguments: fmt::Arguments<'a>) -> Self {
        match arguments.as_str() {
            Some(text) => Self::Borrowed(text),
            None => Self::Formatted(arguments),
        }
    }
}
