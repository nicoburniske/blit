use std::hash::{Hash, Hasher};

use blit::{Atom, AtomCx, Constraints, LogicalRect, Paint, Size};

use crate::{
    color::Color,
    style::{Border, BorderRadius, Fill},
};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Rectangle<'a> {
        new(),
        #[into]
        background: Fill<'a> = Fill::Solid(Color::TRANSPARENT),
        border: Border<'a> = Border::default(),
        radius: BorderRadius = BorderRadius::new(),
        opacity: f32 = 1.0,
    }
}

impl<C> Atom<C> for Rectangle<'static>
where
    C: Paint<Self, Scalar = f32>,
{
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        if self.background != Fill::Solid(Color::TRANSPARENT) || self.border.width > 0.0 {
            cx.context_mut().paint(area, *self);
        }
    }
}

impl Hash for Rectangle<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        [
            self.radius.top_left,
            self.radius.top_right,
            self.radius.bottom_right,
            self.radius.bottom_left,
            self.opacity,
            self.border.width,
        ]
        .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
        .hash(state);
        for fill in [self.background, self.border.fill] {
            std::mem::discriminant(&fill).hash(state);
            match fill {
                Fill::Solid(color) => color.hash(state),
                Fill::LinearGradient(gradient) => {
                    let angle = gradient.angle_degrees;
                    state.write_u32(if angle == 0.0 { 0 } else { angle.to_bits() });
                    gradient.stops.len().hash(state);
                    for stop in gradient.stops {
                        let position = stop.position;
                        state.write_u32(if position == 0.0 { 0 } else { position.to_bits() });
                        stop.color.hash(state);
                    }
                }
            }
        }
    }
}
