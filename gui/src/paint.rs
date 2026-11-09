use std::hash::{Hash, Hasher};

use blit::{Atom, AtomCx, Constraints, LogicalPoint, LogicalRect, Paint, Size};

use crate::{
    color::Color,
    text::{TextOptions, TextRun},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mesh<'a> {
    pub vertices: &'a [MeshVertex],
    pub indices: &'a [u32],
}

impl Hash for Mesh<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.vertices.hash(state);
        self.indices.hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshVertex {
    pub position: LogicalPoint,
    pub color: Color,
}

impl MeshVertex {
    pub const fn new(x: f32, y: f32, color: Color) -> Self {
        Self {
            position: LogicalPoint { x, y },
            color,
        }
    }
}

impl Hash for MeshVertex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        [self.position.x, self.position.y]
            .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
            .hash(state);
        self.color.hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRequest<'a> {
    pub text: TextRun,
    /// one color per span, missing spans repeat the last color
    pub colors: &'a [Color],
    pub offset_x: f32,
    pub options: TextOptions,
}

impl TextRequest<'_> {
    pub fn span_color(&self, span: usize) -> Color {
        self.colors
            .get(span)
            .or(self.colors.last())
            .copied()
            .unwrap_or(Color::TRANSPARENT)
    }
}

impl Hash for TextRequest<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.text.hash(state);
        self.colors.hash(state);
        (if self.offset_x == 0.0 {
            0
        } else {
            self.offset_x.to_bits()
        })
        .hash(state);
        self.options.wrap.hash(state);
        self.options.overflow.hash(state);
        self.options.horizontal_align.hash(state);
        self.options.vertical_align.hash(state);
        self.options.max_lines.hash(state);
    }
}

#[derive(Clone, Copy, Default)]
pub struct Clear;

impl<C: Paint<Clear, Scalar = f32>> Atom<C> for Clear {
    fn measure(&self, _: &mut AtomCx<'_, C>, bounds: Constraints<f32>) -> Size<f32> {
        bounds.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        cx.context_mut().paint(area, *self);
    }
}
