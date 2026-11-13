use std::hash::{Hash, Hasher};

use blit::{Atom, AtomCx, Constraints, LogicalRect, Paint, Sides, Size};

use crate::{color::Color, style::BorderRadius};

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

    pub fn bounds(self, area: LogicalRect) -> LogicalRect {
        if self.inset {
            return area;
        }
        let outset = self.spread + self.blur.max(0.0);
        LogicalRect {
            x: area.x + self.offset_x - outset,
            y: area.y + self.offset_y - outset,
            width: area.width + outset * 2.0,
            height: area.height + outset * 2.0,
        }
    }
}

impl<C> Atom<C> for Shadow
where
    C: Paint<Self, Scalar = f32>,
{
    fn measure(&self, _: &mut AtomCx<'_, C>, _: Constraints<f32>) -> Size<f32> {
        Size::ZERO
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        cx.context_mut().paint(area, *self);
    }

    fn outset(&self) -> Sides<f32> {
        if self.inset {
            return Sides::all(0.0);
        }
        let outset = self.spread + self.blur.max(0.0);
        Sides {
            top: outset - self.offset_y,
            right: outset + self.offset_x,
            bottom: outset + self.offset_y,
            left: outset - self.offset_x,
        }
    }
}

impl Hash for Shadow {
    fn hash<H: Hasher>(&self, state: &mut H) {
        [
            self.radius.top_left,
            self.radius.top_right,
            self.radius.bottom_right,
            self.radius.bottom_left,
            self.offset_x,
            self.offset_y,
            self.blur,
            self.spread,
        ]
        .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
        .hash(state);
        self.color.hash(state);
        self.inset.hash(state);
    }
}
