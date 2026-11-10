use std::hash::{Hash, Hasher};

use blit::{Atom, AtomCx, Constraints, FrameSlice, LogicalRect, Paint, Size, Ui, Widget, state};

use crate::{
    color::Color,
    display_list::{Compare, DisplayList, Slice},
    style::{Border, BorderRadius, ConicGradient, Fill, GradientStop, LinearGradient},
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

impl<C> Widget<C, state::Node> for Rectangle<'_>
where
    C: for<'a> Paint<Rectangle<'a>, Scalar = f32>,
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, C, state::Node>) {
        let [background, border_fill] = [self.background, self.border.fill].map(|fill| match fill {
            Fill::Solid(color) => FrameFill::Solid(color),
            Fill::LinearGradient(gradient) => FrameFill::LinearGradient {
                stops: ui.store_slice(gradient.stops.iter().copied()),
                angle_degrees: gradient.angle_degrees,
            },
            Fill::ConicGradient(gradient) => FrameFill::ConicGradient {
                stops: ui.store_slice(gradient.stops.iter().copied()),
                center: gradient.center,
                angle_degrees: gradient.angle_degrees,
            },
        });
        ui.insert(RectangleAtom {
            background,
            border_fill,
            border_width: self.border.width,
            radius: self.radius,
            opacity: self.opacity,
        });
    }
}

blit::builder! {
    /// rectangle appearance retained in the frame
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct RectangleAtom {
        new(),
        #[into]
        background: FrameFill = FrameFill::Solid(Color::TRANSPARENT),
        #[into]
        border_fill: FrameFill = FrameFill::Solid(Color::TRANSPARENT),
        border_width: f32 = 0.0,
        radius: BorderRadius = BorderRadius::new(),
        opacity: f32 = 1.0,
    }
}

impl<C> Atom<C> for RectangleAtom
where
    C: for<'a> Paint<Rectangle<'a>, Scalar = f32>,
{
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        let rectangle = Rectangle {
            background: self.background.view(cx),
            border: Border {
                width: self.border_width,
                fill: self.border_fill.view(cx),
            },
            radius: self.radius,
            opacity: self.opacity,
        };
        if rectangle.background != Fill::Solid(Color::TRANSPARENT) || rectangle.border.width > 0.0 {
            cx.context_mut().paint(area, rectangle);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameFill {
    Solid(Color),
    LinearGradient {
        stops: FrameSlice<GradientStop>,
        angle_degrees: f32,
    },
    ConicGradient {
        stops: FrameSlice<GradientStop>,
        center: [f32; 2],
        angle_degrees: f32,
    },
}

impl FrameFill {
    fn view<'a, C: blit::Context>(&self, cx: &AtomCx<'a, C>) -> Fill<'a> {
        match *self {
            Self::Solid(color) => Fill::Solid(color),
            Self::LinearGradient { stops, angle_degrees } => Fill::LinearGradient(LinearGradient {
                stops: cx.slice(stops),
                angle_degrees,
            }),
            Self::ConicGradient {
                stops,
                center,
                angle_degrees,
            } => Fill::ConicGradient(ConicGradient {
                stops: cx.slice(stops),
                center,
                angle_degrees,
            }),
        }
    }
}

impl From<Color> for FrameFill {
    fn from(color: Color) -> Self {
        Self::Solid(color)
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
            let (stops, angle) = match fill {
                Fill::Solid(color) => {
                    color.hash(state);
                    continue;
                }
                Fill::LinearGradient(gradient) => (gradient.stops, gradient.angle_degrees),
                Fill::ConicGradient(gradient) => {
                    gradient
                        .center
                        .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
                        .hash(state);
                    (gradient.stops, gradient.angle_degrees)
                }
            };
            state.write_u32(if angle == 0.0 { 0 } else { angle.to_bits() });
            stops.len().hash(state);
            for stop in stops {
                let position = stop.position;
                state.write_u32(if position == 0.0 { 0 } else { position.to_bits() });
                stop.color.hash(state);
            }
        }
    }
}

#[doc(hidden)]
pub struct Stored {
    pub area: LogicalRect,
    pub background: StoredFill,
    pub border_fill: StoredFill,
    pub border_width: f32,
    pub radius: BorderRadius,
    pub opacity: f32,
}

impl Stored {
    #[inline]
    pub fn new(area: LogicalRect, rectangle: Rectangle<'_>, list: &mut DisplayList) -> Self {
        Self {
            area,
            background: StoredFill::new(rectangle.background, list),
            border_fill: StoredFill::new(rectangle.border.fill, list),
            border_width: rectangle.border.width,
            radius: rectangle.radius,
            opacity: rectangle.opacity,
        }
    }

    #[inline]
    pub fn view<'a>(&self, list: &'a DisplayList) -> Rectangle<'a> {
        Rectangle {
            background: self.background.view(list),
            border: Border {
                width: self.border_width,
                fill: self.border_fill.view(list),
            },
            radius: self.radius,
            opacity: self.opacity,
        }
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

pub enum StoredFill {
    Solid(Color),
    LinearGradient {
        stops: Slice<GradientStop>,
        angle_degrees: f32,
    },
    ConicGradient {
        stops: Slice<GradientStop>,
        center: [f32; 2],
        angle_degrees: f32,
    },
}

impl StoredFill {
    #[inline]
    fn new(fill: Fill<'_>, list: &mut DisplayList) -> Self {
        match fill {
            Fill::Solid(color) => Self::Solid(color),
            Fill::LinearGradient(gradient) => Self::LinearGradient {
                stops: list.store_slice(gradient.stops.iter().copied()),
                angle_degrees: gradient.angle_degrees,
            },
            Fill::ConicGradient(gradient) => Self::ConicGradient {
                stops: list.store_slice(gradient.stops.iter().copied()),
                center: gradient.center,
                angle_degrees: gradient.angle_degrees,
            },
        }
    }

    #[inline]
    fn view<'a>(&self, list: &'a DisplayList) -> Fill<'a> {
        match *self {
            Self::Solid(color) => Fill::Solid(color),
            Self::LinearGradient { stops, angle_degrees } => Fill::LinearGradient(LinearGradient {
                stops: list.slice(stops),
                angle_degrees,
            }),
            Self::ConicGradient {
                stops,
                center,
                angle_degrees,
            } => Fill::ConicGradient(ConicGradient {
                stops: list.slice(stops),
                center,
                angle_degrees,
            }),
        }
    }
}
