use blit::{Atom, AtomCx, Constraints, PhysicalRect, Size};

use crate::{
    Context,
    color::Color,
    text::{TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextWrap},
};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Text {
        new(text: TextRunId),
        color: Color = Color::Reset,
        attributes: TextAttributes = TextAttributes::NONE,
        options: TextOptions = TextOptions::new(),
    }
}

blit::atom_widget!(Text);

impl<C: Context> Atom<C> for Text {
    fn measure(&self, cx: &mut AtomCx<'_, C>, constraints: Constraints<i32>) -> Size<i32> {
        let mut request = TextLayoutRequest::new(self.text).wrap(self.options.wrap);
        if self.options.wrap != TextWrap::None && constraints.max.width != i32::MAX {
            request = request.max_width(constraints.max.width);
        }
        if let Some(max_lines) = self.options.max_lines {
            request = request.max_lines(max_lines);
        }
        constraints.constrain(cx.context_mut().as_mut().renderer_mut().measure_text(&request))
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: PhysicalRect) {
        cx.context_mut().as_mut().paint_text(
            TextRequest::new(self.text, area)
                .color(self.color)
                .attributes(self.attributes)
                .options(self.options),
        );
    }
}
