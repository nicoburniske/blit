use std::hash::{Hash, Hasher};

use blit::{LogicalRect, Paint, Ui, Widget, state};

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

impl<C> Widget<C, state::Insert> for Shadow
where
    C: Paint<Self, Scalar = f32>,
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, C, state::Insert>) {
        ui.paint(move |cx, area| cx.context_mut().paint(area, &self));
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
