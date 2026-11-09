use std::fmt;

use blit::{Atom, AtomCx, Constraints, PhysicalRect, Size, Widget, state};

use crate::{
    Context,
    color::Color,
    text::{Span, TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextWrap},
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

impl<C> Widget<C, state::Node> for Text<'_>
where
    C: Context,
{
    type Response = ();

    fn build(self, mut ui: blit::Ui<'_, C, state::Node>) {
        let run = if let Some(spans) = self.spans {
            ui.context_mut().as_mut().renderer.rich_text(spans)
        } else {
            let renderer = &mut ui.context_mut().as_mut().renderer;
            match self.text {
                TextSource::Borrowed(text) => renderer.text_run(text),
                TextSource::Formatted(arguments) => renderer.text_run_fmt(arguments),
            }
        };
        ui.insert(
            TextAtom::new(run)
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

impl<'a, T> From<&'a T> for TextSource<'a>
where
    T: AsRef<str> + ?Sized,
{
    fn from(text: &'a T) -> Self {
        Self::Borrowed(T::as_ref(text))
    }
}

impl<'a, T> From<&'a mut T> for TextSource<'a>
where
    T: AsRef<str> + ?Sized,
{
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

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct TextAtom {
        new(text: TextRunId),
        color: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
        options: TextOptions = TextOptions::new(),
    }
}

impl<C> Atom<C> for TextAtom
where
    C: Context,
{
    fn measure(&self, cx: &mut AtomCx<'_, C>, constraints: Constraints<i32>) -> Size<i32> {
        let mut request = TextLayoutRequest::new(self.text).wrap(self.options.wrap);
        if self.options.wrap != TextWrap::None && constraints.max.width != i32::MAX {
            request = request.max_width(constraints.max.width);
        }
        if let Some(max_lines) = self.options.max_lines {
            request = request.max_lines(max_lines);
        }
        constraints.constrain(cx.context_mut().as_mut().renderer.measure_text(&request))
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: PhysicalRect) {
        cx.context_mut().as_mut().renderer.paint_text(
            TextRequest::new(self.text, area)
                .color(self.color)
                .attributes(self.attributes)
                .options(self.options),
        );
    }
}
