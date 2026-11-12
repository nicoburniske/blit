use std::{
    hash::{Hash, Hasher},
    ops::Deref,
};

use blit::{FrameSlice, FrameStore, LogicalPoint, LogicalRect, Paint};

use crate::{
    color::Color,
    display_list::{Compare, DisplayList},
    rectangle::FrameFill,
    style::Fill,
};

// todo: dashes

blit::builder! {
    /// filled or stroked vector path in logical coordinates
    ///
    /// subpaths are closed implicitly when filled
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Path<'a> {
        new(verbs: &'a [Verb], points: &'a [LogicalPoint]),
        #[into]
        fill: Fill<'a> = Fill::Solid(Color::TRANSPARENT),
        rule: FillRule = FillRule::NonZero,
        /// paints the stroke outline with the fill instead of the interior
        #[into]
        stroke: Option<Stroke> = None,
        opacity: f32 = 1.0,
    }
}

impl Path<'_> {
    /// stores borrowed geometry in the context and paints the result
    pub fn paint<C: FrameStore + Paint<Stored, Scalar = f32>>(self, context: &mut C) {
        if self.fill == Fill::Solid(Color::TRANSPARENT) || self.opacity <= 0.0 {
            return;
        }
        let Some(bounds) = self.bounds() else {
            return;
        };
        let mut verbs = context.slice(self.verbs.len());
        verbs.extend(self.verbs.iter().copied());
        let verbs = verbs.finish();
        let mut points = context.slice(self.points.len());
        points.extend(self.points.iter().copied());
        let points = points.finish();
        let stored = Stored {
            verbs,
            points,
            fill: FrameFill::new(self.fill, context),
            rule: self.rule,
            stroke: self.stroke,
            opacity: self.opacity,
        };
        context.paint(bounds, stored);
    }

    /// conservative bounds of all points or none when the geometry is malformed or empty
    pub fn bounds(&self) -> Option<LogicalRect> {
        let count = self.verbs.iter().map(|verb| verb.points()).sum::<usize>();
        if count != self.points.len() || self.points.is_empty() {
            return None;
        }
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for point in self.points {
            if !point.x.is_finite() || !point.y.is_finite() {
                return None;
            }
            min = [min[0].min(point.x), min[1].min(point.y)];
            max = [max[0].max(point.x), max[1].max(point.y)];
        }
        // miters reach furthest and square caps reach half the width diagonally
        let outset = self.stroke.map_or(0.0, |stroke| {
            let reach = match stroke.join {
                Join::Miter => stroke.miter_limit.max(std::f32::consts::SQRT_2),
                Join::Bevel | Join::Round => std::f32::consts::SQRT_2,
            };
            0.5 * stroke.width * reach
        });
        Some(LogicalRect {
            x: min[0] - outset,
            y: min[1] - outset,
            width: max[0] - min[0] + 2.0 * outset,
            height: max[1] - min[1] + 2.0 * outset,
        })
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Stroke {
        new(width: f32),
        join: Join = Join::Miter,
        cap: Cap = Cap::Butt,
        /// miter length over width before a miter join falls back to bevel
        miter_limit: f32 = 4.0,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Join {
    Bevel,
    #[default]
    Miter,
    Round,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Cap {
    #[default]
    Butt,
    Square,
    Round,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Verb {
    Move,
    Line,
    Quad,
    Cubic,
    Close,
}

impl Verb {
    pub const fn points(self) -> usize {
        match self {
            Self::Move | Self::Line => 1,
            Self::Quad => 2,
            Self::Cubic => 3,
            Self::Close => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// builds path geometry in owned or borrowed buffers
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PathBuilder<V = Vec<Verb>, P = Vec<LogicalPoint>> {
    pub verbs: V,
    pub points: P,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.verbs.clear();
        self.points.clear();
    }
}

impl PathBuilder<blit::Scratch<'_, Verb>, blit::Scratch<'_, LogicalPoint>> {
    pub fn clear(&mut self) {
        self.verbs.clear();
        self.points.clear();
    }
}

impl<V, P> PathBuilder<V, P>
where
    V: Deref<Target = [Verb]> + Extend<Verb>,
    P: Deref<Target = [LogicalPoint]> + Extend<LogicalPoint>,
{
    pub fn path(&self) -> Path<'_> {
        Path::new(&self.verbs, &self.points)
    }

    pub fn move_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.verbs.extend([Verb::Move]);
        self.points.extend([LogicalPoint { x, y }]);
        self
    }

    pub fn line_to(&mut self, x: f32, y: f32) -> &mut Self {
        self.verbs.extend([Verb::Line]);
        self.points.extend([LogicalPoint { x, y }]);
        self
    }

    pub fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) -> &mut Self {
        self.verbs.extend([Verb::Quad]);
        self.points
            .extend([LogicalPoint { x: cx, y: cy }, LogicalPoint { x, y }]);
        self
    }

    pub fn cubic_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) -> &mut Self {
        self.verbs.extend([Verb::Cubic]);
        self.points.extend([
            LogicalPoint { x: c1x, y: c1y },
            LogicalPoint { x: c2x, y: c2y },
            LogicalPoint { x, y },
        ]);
        self
    }

    pub fn close(&mut self) -> &mut Self {
        self.verbs.extend([Verb::Close]);
        self
    }

    /// clockwise rectangle subpath
    pub fn rect(&mut self, area: LogicalRect) -> &mut Self {
        let (right, bottom) = (area.x + area.width, area.y + area.height);
        self.move_to(area.x, area.y)
            .line_to(right, area.y)
            .line_to(right, bottom)
            .line_to(area.x, bottom)
            .close()
    }

    /// clockwise circle subpath from four cubic arcs
    pub fn circle(&mut self, x: f32, y: f32, radius: f32) -> &mut Self {
        let k = radius * 0.552_284_8;
        self.move_to(x + radius, y)
            .cubic_to(x + radius, y + k, x + k, y + radius, x, y + radius)
            .cubic_to(x - k, y + radius, x - radius, y + k, x - radius, y)
            .cubic_to(x - radius, y - k, x - k, y - radius, x, y - radius)
            .cubic_to(x + k, y - radius, x + radius, y - k, x + radius, y)
            .close()
    }
}

impl Hash for Path<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.verbs.hash(state);
        self.points.len().hash(state);
        for point in self.points {
            [point.x, point.y]
                .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
                .hash(state);
        }
        self.fill.hash(state);
        self.rule.hash(state);
        if let Some(stroke) = self.stroke {
            [stroke.width, stroke.miter_limit]
                .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
                .hash(state);
            (stroke.join, stroke.cap).hash(state);
        }
        state.write_u32(if self.opacity == 0.0 { 0 } else { self.opacity.to_bits() });
    }
}

/// path slices stored in a rendering context
pub struct Stored {
    pub verbs: FrameSlice<Verb>,
    pub points: FrameSlice<LogicalPoint>,
    pub fill: FrameFill,
    pub rule: FillRule,
    pub stroke: Option<Stroke>,
    pub opacity: f32,
}

impl Stored {
    pub fn view<'a>(&self, list: &'a DisplayList) -> Path<'a> {
        Path {
            verbs: list.get(self.verbs),
            points: list.get(self.points),
            fill: self.fill.view(list),
            rule: self.rule,
            stroke: self.stroke,
            opacity: self.opacity,
        }
    }
}

impl Compare for Stored {
    fn equal(a: &Self, left: &DisplayList, b: &Self, right: &DisplayList) -> bool {
        a.view(left) == b.view(right)
    }

    fn hash(input: &Self, list: &DisplayList, state: &mut impl Hasher) {
        input.view(list).hash(state);
    }
}
