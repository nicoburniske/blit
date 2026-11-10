use blit::{LogicalRect, Paint};
use blit_gui::mesh::{self, Mesh};

use crate::{MeshData, Renderer};

impl Paint<Mesh<'_>> for Renderer {
    fn paint(&mut self, bounds: LogicalRect, mesh: Mesh<'_>) {
        if mesh.indices.is_empty() || !mesh.is_valid() {
            return;
        }
        let value = mesh::Stored {
            vertices: self.list.store_slice(mesh.vertices.iter().copied()),
            indices: self.list.store_slice(mesh.indices.iter().copied()),
        };
        self.record_native(bounds, self.paint_clip, value, |value, list, renderer, _, clip| {
            let mesh = value.view(list);

            let scale = renderer.scale;
            let vertex_start = renderer.mesh_data.len() as u32;
            renderer.mesh_data.extend(mesh.vertices.iter().map(|vertex| {
                MeshData([
                    (vertex.position.x * scale).to_bits(),
                    (vertex.position.y * scale).to_bits(),
                    u32::from_le_bytes([
                        vertex.color.red,
                        vertex.color.green,
                        vertex.color.blue,
                        vertex.color.alpha,
                    ]),
                    clip,
                ])
            }));
            assert!(
                renderer.mesh_data.len() <= u32::MAX as usize,
                "too many GPU mesh vertices"
            );
            for triangle in mesh.indices.as_chunks::<3>().0 {
                let [first, second, third] = triangle.map(|index| vertex_start + index);
                renderer.push_mesh_primitive([first, second, third, third]);
            }
        });
    }
}
