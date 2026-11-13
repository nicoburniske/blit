use std::hash::{Hash, Hasher};

use blit::{Atom, AtomCx, Constraints, FrameSlice, HasFrameStore, LogicalRect, Paint, Size, Ui, Widget, state};

use crate::{
    color::Color,
    display_list::{Compare, DisplayList},
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
    C: HasFrameStore + Paint<RectangleAtom, Scalar = f32>,
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, C, state::Node>) {
        let background = FrameFill::new(self.background, ui.context_mut());
        let border_fill = FrameFill::new(self.border.fill, ui.context_mut());
        ui.insert(RectangleAtom {
            background,
            border_fill,
            border_width: self.border.width,
            radius: self.radius,
            opacity: self.opacity,
        });
    }
}

impl Rectangle<'_> {
    /// stores borrowed fills in the context and paints the result
    pub fn paint<C: HasFrameStore + Paint<RectangleAtom, Scalar = f32>>(self, area: LogicalRect, context: &mut C) {
        let rectangle = RectangleAtom {
            background: FrameFill::new(self.background, context),
            border_fill: FrameFill::new(self.border.fill, context),
            border_width: self.border.width,
            radius: self.radius,
            opacity: self.opacity,
        };
        context.paint(area, rectangle);
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
    C: Paint<Self, Scalar = f32>,
{
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        if self.background != FrameFill::Solid(Color::TRANSPARENT) || self.border_width > 0.0 {
            cx.context_mut().paint(area, *self);
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
        self.background.hash(state);
        self.border.fill.hash(state);
    }
}

impl RectangleAtom {
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

impl Compare for RectangleAtom {
    #[inline]
    fn equal(a: &Self, left: &DisplayList, b: &Self, right: &DisplayList) -> bool {
        a.view(left) == b.view(right)
    }

    fn hash(value: &Self, list: &DisplayList, state: &mut impl Hasher) {
        value.view(list).hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameFill {
    Solid(Color),
    LinearGradient {
        stops: FrameSlice<GradientStop>,
        angle: f32,
    },
    ConicGradient {
        stops: FrameSlice<GradientStop>,
        center: [f32; 2],
        angle: f32,
    },
}

impl FrameFill {
    #[inline]
    pub(crate) fn new(fill: Fill<'_>, store: &mut impl HasFrameStore) -> Self {
        match fill {
            Fill::Solid(color) => Self::Solid(color),
            Fill::LinearGradient(gradient) => Self::LinearGradient {
                stops: {
                    let mut stops = store.store_mut().slice(gradient.stops.len());
                    stops.extend(gradient.stops.iter().copied());
                    stops.finish()
                },
                angle: gradient.angle,
            },
            Fill::ConicGradient(gradient) => Self::ConicGradient {
                stops: {
                    let mut stops = store.store_mut().slice(gradient.stops.len());
                    stops.extend(gradient.stops.iter().copied());
                    stops.finish()
                },
                center: gradient.center,
                angle: gradient.angle,
            },
        }
    }

    #[inline]
    pub(crate) fn view<'a>(&self, list: &'a DisplayList) -> Fill<'a> {
        match *self {
            Self::Solid(color) => Fill::Solid(color),
            Self::LinearGradient { stops, angle } => Fill::LinearGradient(LinearGradient {
                stops: list.store().view(stops),
                angle,
            }),
            Self::ConicGradient { stops, center, angle } => Fill::ConicGradient(ConicGradient {
                stops: list.store().view(stops),
                center,
                angle,
            }),
        }
    }
}
