use blit::{LogicalPoint, LogicalRect};

use crate::{
    color::Color,
    style::{Border, BorderRadius, Fill},
};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Rectangle<'a> {
        new(area: LogicalRect),
        #[into]
        background: Fill<'a> = Fill::Solid(Color::TRANSPARENT),
        border: Border<'a> = Border::default(),
        radius: BorderRadius = BorderRadius::new(),
        opacity: f32 = 1.0,
    }
}

impl Hash for Rectangle<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        [
            self.area.x,
            self.area.y,
            self.area.width,
            self.area.height,
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
use std::hash::{Hash, Hasher};
