use blit::{Atom, AtomCx, Constraints, LogicalRect, Paint, Size};

use crate::{
    color::Color,
    paint,
    style::{Border, BorderRadius, Fill},
};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Rectangle {
        new(),
        #[into]
        background: Fill<'static> = Fill::Solid(Color::TRANSPARENT),
        border: Border<'static> = Border::default(),
        radius: BorderRadius = BorderRadius::new(),
        opacity: f32 = 1.0,
    }
}

impl<C: for<'a> Paint<paint::Rectangle<'a>, Scalar = f32>> Atom<C> for Rectangle {
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        if self.background != Fill::Solid(Color::TRANSPARENT) || self.border.width > 0.0 {
            cx.context_mut().paint(
                area,
                paint::Rectangle {
                    area,
                    background: self.background,
                    border: self.border,
                    radius: self.radius,
                    opacity: self.opacity,
                },
            );
        }
    }
}
