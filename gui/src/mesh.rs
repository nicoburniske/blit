use std::hash::{Hash, Hasher};

use blit::{FrameSlice, HasFrameStore, LogicalPoint, LogicalRect, Paint};

use crate::{
    color::Color,
    display_list::{Compare, DisplayList},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mesh<'a> {
    pub vertices: &'a [MeshVertex],
    pub indices: &'a [u32],
}

impl Mesh<'_> {
    /// stores borrowed geometry in the context and paints the result
    pub fn paint<C: HasFrameStore + Paint<Stored, Scalar = f32>>(self, bounds: LogicalRect, context: &mut C) {
        if self.indices.is_empty() || !self.is_valid() {
            return;
        }
        let mut vertices = context.store_mut().slice(self.vertices.len());
        vertices.extend(self.vertices.iter().copied());
        let vertices = vertices.finish();
        let mut indices = context.store_mut().slice(self.indices.len());
        indices.extend(self.indices.iter().copied());
        let stored = Stored {
            vertices,
            indices: indices.finish(),
        };
        context.paint(bounds, stored);
    }

    pub fn is_valid(&self) -> bool {
        self.indices.len().is_multiple_of(3)
            && self.indices.iter().all(|index| (*index as usize) < self.vertices.len())
            && self
                .vertices
                .iter()
                .all(|vertex| vertex.position.x.is_finite() && vertex.position.y.is_finite())
    }
}

impl Hash for Mesh<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.vertices.hash(state);
        self.indices.hash(state);
    }
}

/// mesh slices stored in a rendering context
pub struct Stored {
    pub vertices: FrameSlice<MeshVertex>,
    pub indices: FrameSlice<u32>,
}

impl Stored {
    pub fn view<'a>(&self, list: &'a DisplayList) -> Mesh<'a> {
        Mesh {
            vertices: list.store().view(self.vertices),
            indices: list.store().view(self.indices),
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
