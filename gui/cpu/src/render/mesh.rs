use blit::{LogicalRect, Paint, PhysicalRect, internal::bump::Offset};
use blit_gui::{
    damage::DamageTracker,
    display_list::DisplayList,
    mesh::{self, Mesh},
};

use super::triangle;
use crate::{
    Pixel, PixelBuffer, PixelSpan, RenderStrategy, Renderer, handler::ErasedCommand, strategy::command::Payload,
};

impl<B, S, D> Paint<Mesh<'_>> for Renderer<B, S, D>
where
    B: PixelBuffer,
    S: RenderStrategy<B>,
    D: DamageTracker,
{
    fn paint(&mut self, bounds: LogicalRect, mesh: Mesh<'_>) {
        let mut recording = self.record(bounds);
        if mesh.indices.is_empty() || !mesh.is_valid() {
            return;
        }
        let vertices = recording.list.store_slice(mesh.vertices.iter().copied());
        let indices = recording.list.store_slice(mesh.indices.iter().copied());
        let value = mesh::Stored { vertices, indices };
        recording.record_input(
            recording.bounds,
            recording.clip,
            value,
            |_, mesh_offset, record, _, context| {
                let Some(bounds) = context
                    .clips
                    .bounds(record.clip, record.bounds)
                    .and_then(|bounds| bounds.intersection(context.screen))
                else {
                    return;
                };
                let target = &mut *context.commands;
                let index = target.mesh_count;
                if index == target.meshes.len() {
                    target
                        .meshes
                        .push(crate::render::mesh::Prepared::new(Offset::new(mesh_offset)));
                } else {
                    target.meshes[index].source = Offset::new(mesh_offset);
                }
                target.mesh_count += 1;
                let call = ErasedCommand::ready(&mut target.prepared, Payload::Mesh(index as u32));
                target.push(call, bounds, record.clip);
            },
        );
    }
}

pub struct Prepared {
    pub source: Offset<mesh::Stored>,
    pub triangles: Vec<(PhysicalRect, Triangle)>,
    pub starts: Vec<u32>,
    active: Vec<u32>,
    next: usize,
    line: i32,
}

impl Prepared {
    pub fn new(source: Offset<mesh::Stored>) -> Self {
        Self {
            source,
            triangles: Vec::new(),
            starts: Vec::new(),
            active: Vec::new(),
            next: 0,
            line: -1,
        }
    }

    pub fn draw_line<P: Pixel>(
        &mut self,
        list: &DisplayList,
        scale: f32,
        line: i32,
        clip: PhysicalRect,
        coverage: u8,
        row: PixelSpan<'_, P>,
    ) {
        if self.line != line {
            self.active.retain(|id| {
                let bounds = self.triangles[*id as usize].0;
                bounds.y + bounds.height > line
            });
            while self.next < self.starts.len() {
                let id = self.starts[self.next];
                let bounds = self.triangles[id as usize].0;
                if bounds.y > line {
                    break;
                }
                if bounds.y + bounds.height > line {
                    let position = self.active.binary_search(&id).unwrap_err();
                    self.active.insert(position, id);
                }
                self.next += 1;
            }
            self.line = line;
        }
        // safety: mesh recording stores an offset to blit_gui::mesh::Stored in this list
        let source = unsafe { list.data().get(self.source) };
        let mesh = source.view(list);
        for &id in &self.active {
            let (bounds, triangle) = &mut self.triangles[id as usize];
            let Some(clip) = bounds.intersection(clip) else {
                continue;
            };
            if let Triangle::Pending(index) = *triangle {
                let vertices =
                    mesh.indices.as_chunks::<3>().0[index as usize].map(|index| mesh.vertices[index as usize]);
                let points = vertices.map(|vertex| [vertex.position.x * scale, vertex.position.y * scale]);
                *triangle = triangle::Prepared::new(points, vertices.map(|vertex| vertex.color))
                    .map(Triangle::Ready)
                    .unwrap_or(Triangle::Empty);
            }
            if let Triangle::Ready(triangle) = triangle {
                triangle.draw_line(
                    line,
                    clip,
                    coverage,
                    PixelSpan {
                        x: row.x,
                        pixels: row.pixels,
                    },
                );
            }
        }
    }

    pub fn clear(&mut self) {
        self.triangles.clear();
        self.starts.clear();
        self.active.clear();
        self.next = 0;
        self.line = -1;
    }
}

pub enum Triangle {
    Pending(u32),
    Ready(triangle::Prepared),
    Empty,
}
