use crate::{
    color::Color,
    text::{TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextWrap},
};
use blit::{Atom, Constraints, LogicalRect, Size};

use crate::TuiContext;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Text {
        new(text: TextRunId),
        color: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
        options: TextOptions = TextOptions::new(),
    }
}

impl Atom<TuiContext> for Text {
    fn measure(&self, context: &mut TuiContext, constraints: Constraints) -> Size {
        let mut request = TextLayoutRequest::new(self.text).wrap(self.options.wrap);
        if self.options.wrap != TextWrap::None && constraints.max.width.is_finite() {
            request = request.max_width(constraints.max.width);
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

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        area
    }
}
