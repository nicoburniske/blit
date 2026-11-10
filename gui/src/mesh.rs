use std::hash::{Hash, Hasher};

use blit::LogicalPoint;

use crate::{
    color::Color,
    display_list::{Compare, DisplayList, Slice},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mesh<'a> {
    pub vertices: &'a [MeshVertex],
    pub indices: &'a [u32],
}

impl Mesh<'_> {
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

/// mesh slices owned by a display list
pub struct Stored {
    pub vertices: Slice<MeshVertex>,
    pub indices: Slice<u32>,
}

impl Stored {
    pub fn view<'a>(&self, list: &'a DisplayList) -> Mesh<'a> {
        Mesh {
            vertices: list.slice(self.vertices),
            indices: list.slice(self.indices),
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
