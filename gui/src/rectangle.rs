use std::hash::{Hash, Hasher};

use blit::{Atom, AtomCx, Constraints, LogicalRect, Paint, Size};

use crate::{
    color::Color,
    display_list::{Compare, DisplayList, Slice},
    style::{Border, BorderRadius, Fill, GradientStop, LinearGradient},
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

#[doc(hidden)]
pub struct Stored {
    pub area: LogicalRect,
    rectangle: Rectangle<'static>,
    stops: [Option<Slice<GradientStop>>; 2],
}

impl Stored {
    #[inline]
    pub fn new(area: LogicalRect, rectangle: Rectangle<'_>, list: &mut DisplayList) -> Self {
        let [(background, background_stops), (border, border_stops)] = [rectangle.background, rectangle.border.fill]
            .map(|fill| match fill {
                Fill::Solid(color) => (Fill::Solid(color), None),
                Fill::LinearGradient(gradient) => {
                    assert!(
                        gradient.stops.len() >= 2
                            && gradient.angle_degrees.is_finite()
                            && gradient
                                .stops
                                .iter()
                                .all(|stop| stop.position.is_finite() && (0.0..=1.0).contains(&stop.position))
                            && gradient
                                .stops
                                .windows(2)
                                .all(|stops| stops[0].position < stops[1].position),
                        "invalid linear gradient"
                    );
                    (
                        Fill::LinearGradient(LinearGradient { stops: &[], ..gradient }),
                        Some(list.store_slice(gradient.stops.iter().copied())),
                    )
                }
            });
        Self {
            area,
            rectangle: Rectangle {
                background,
                border: Border {
                    fill: border,
                    ..rectangle.border
                },
                ..rectangle
            },
            stops: [background_stops, border_stops],
        }
    }

    #[inline]
    pub fn view<'a>(&self, list: &'a DisplayList) -> Rectangle<'a> {
        let mut rectangle = self.rectangle;
        for (fill, stops) in [&mut rectangle.background, &mut rectangle.border.fill]
            .into_iter()
            .zip(self.stops)
        {
            if let Fill::LinearGradient(gradient) = fill {
                gradient.stops = list.slice(stops.unwrap());
            }
        }
        rectangle
    }
}

impl Compare for Stored {
    #[inline]
    fn equal(a: &Self, left: &DisplayList, b: &Self, right: &DisplayList) -> bool {
        a.area == b.area && a.view(left) == b.view(right)
    }

    fn hash(value: &Self, list: &DisplayList, state: &mut impl Hasher) {
        [value.area.x, value.area.y, value.area.width, value.area.height]
            .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
            .hash(state);
        value.view(list).hash(state);
    }
}
