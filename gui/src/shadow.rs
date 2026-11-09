use blit::{Atom, AtomCx, Constraints, LogicalRect, Paint, Size};

use crate::{color::Color, paint::BoxShadow, style::BorderRadius};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Shadow {
        new(color: Color),
        radius: BorderRadius = BorderRadius::new(),
        offset_x: f32 = 0.0,
        offset_y: f32 = 0.0,
        blur: f32 = 0.0,
        spread: f32 = 0.0,
        inset: bool = false,
    }
}

impl Shadow {
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }

    fn command(&self, area: LogicalRect) -> BoxShadow {
        BoxShadow::new(area, self.color)
            .radius(self.radius)
            .offset(self.offset_x, self.offset_y)
            .blur(self.blur)
            .spread(self.spread)
            .inset(self.inset)
    }
}

impl<C: Paint<BoxShadow, Scalar = f32>> Atom<C> for Shadow {
    fn measure(&self, _: &mut AtomCx<'_, C>, _: Constraints<f32>) -> Size<f32> {
        Size::ZERO
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        let command = self.command(area);
        cx.context_mut().paint(command.bounds(), command);
    }

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        self.command(area).bounds()
    }
}
