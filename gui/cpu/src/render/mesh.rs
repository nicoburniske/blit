use blit::{PhysicalRect, internal::bump::Offset};
use blit_gui::display_list::DisplayList;

use super::triangle;
use crate::{Pixel, PixelSpan, paint::StoredMesh};

pub struct Prepared {
    pub source: Offset<StoredMesh>,
    pub triangles: Vec<(PhysicalRect, Triangle)>,
    pub starts: Vec<u32>,
    active: Vec<u32>,
    next: usize,
    line: i32,
}

impl Prepared {
    pub fn new(source: Offset<StoredMesh>) -> Self {
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
        // safety: mesh recording stores an offset to StoredMesh in this list
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
                let points =
                    vertices.map(|vertex| [vertex.position.x * source.scale, vertex.position.y * source.scale]);
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
