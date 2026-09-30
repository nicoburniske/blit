use blit::{Atom, Constraints, IntrinsicQuery, IntrinsicSize, LogicalRect, LogicalSize};

use crate::{
    TuiContext,
    color::Color,
    text::{TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextWrap},
};

blit::builder! {
    #[const]
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Text {
        new(text: TextRunId),
        color: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
        options: TextOptions = TextOptions::new(),
    }
}

impl Atom<TuiContext> for Text {
    fn intrinsic(&self, context: &mut TuiContext, query: IntrinsicQuery) -> IntrinsicSize {
        context.renderer_mut().intrinsic_text(
            &TextLayoutRequest {
                text: self.text,
                wrap: self.options.wrap,
                max_columns: None,
                max_lines: self.options.max_lines,
            },
            query,
        )
    }

    fn measure(&self, context: &mut TuiContext, constraints: Constraints) -> LogicalSize {
        let mut request = TextLayoutRequest::new(self.text).wrap(self.options.wrap);
        if self.options.wrap != TextWrap::None && constraints.max.width.is_finite() {
            request = request.max_columns(constraints.max.width.floor().max(0.0) as usize);
        }
        if let Some(max_lines) = self.options.max_lines {
            request = request.max_lines(max_lines);
        }
        constraints.constrain(context.renderer_mut().measure_text(&request))
    }

    fn paint(&self, context: &mut TuiContext, area: LogicalRect) {
        context.paint_text(
            TextRequest::new(self.text, area)
                .color(self.color)
                .attributes(self.attributes)
                .options(self.options),
        );
    }
}
