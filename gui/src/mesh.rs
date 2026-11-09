use std::hash::{Hash, Hasher};

use blit::LogicalPoint;

use crate::color::Color;

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
