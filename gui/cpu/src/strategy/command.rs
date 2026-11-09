use std::{ops::Range, ptr::NonNull};

use blit::{PhysicalRect, internal::bump::Bump};
use blit_gui::{color::Color, display_list::DisplayList};

use super::clip::ClipId;
use crate::{
    DamageTracker, DrawContext, Pixel, StoredImage,
    paint::ErasedCommand,
    raster::DrawTarget,
    render::{
        fill::{Gradients, PreparedFill},
        image_patch, mesh, rectangle,
    },
    text::{PreparedRuns, TextRenderer},
};

pub type CommandId = u32;

pub struct RasterCommands<P> {
    pub scale: f32,
    pub metadata: Vec<Metadata>,
    pub entries: Vec<ErasedCommand<P>>,
    pub data: Bump,
    pub prepared: Bump,
    pub range: Range<CommandId>,
    pub gradients: Gradients,
    pub meshes: Vec<mesh::Prepared>,
    pub mesh_count: usize,
    pub passes: Vec<CommandId>,
}

impl<P> Default for RasterCommands<P> {
    fn default() -> Self {
        Self {
            scale: 1.0,
            metadata: Vec::new(),
            entries: Vec::new(),
            data: Bump::default(),
            prepared: Bump::default(),
            range: 0..0,
            gradients: Gradients::default(),
            meshes: Vec::new(),
            mesh_count: 0,
            passes: Vec::new(),
        }
    }
}

impl<P: Pixel> RasterCommands<P> {
    pub fn len(&self) -> CommandId {
        CommandId::try_from(self.metadata.len()).expect("too many cpu commands in one frame")
    }

    #[inline(always)]
    pub fn prepare(
        &mut self,
        id: CommandId,
        list: &DisplayList,
        images: &mut blit_arena::Arena<StoredImage>,
        text: &mut TextRenderer,
    ) -> &Payload {
        let command = &mut self.metadata[id as usize];
        let input = &mut self.entries[id as usize];
        if input.draw.is_some() {
            return &Payload::Custom;
        }
        if !input.is_pending() {
            // safety: entries without a draw callback prepare Payload values
            return unsafe { input.get(&self.prepared) };
        }
        input.prepare(
            &self.data,
            &mut self.prepared,
            &mut DrawContext {
                scale: self.scale,
                bounds: command.bounds,
                list,
                text,
                gradients: &mut self.gradients,
                images,
                source: None,
                target: DrawTarget::Prepare,
            },
        );
        // safety: entries without a draw callback prepare Payload values
        if let Payload::Text(_, bounds) = unsafe { input.get(&self.prepared) } {
            command.bounds = bounds.intersection(command.bounds).unwrap_or_default();
        }
        let Payload::Rectangle(first) = self.get(id) else {
            return self.get(id);
        };
        let first = *first;
        let next = id + 1;
        if matches!(first.background, PreparedFill::Solid(color) if color.alpha == 255)
            && first.radii.is_zero()
            && first.border_width == 0
            && self.clip(id) == 0
            && next < self.range.end
            && self.clip(next) == 0
            && self.bounds(id) == self.bounds(next)
            && let Payload::Rectangle(second) = self.prepare(next, list, images, text)
            && let Some(pair) = first.fuse(second)
        {
            self.entries[id as usize] = ErasedCommand::ready(&mut self.prepared, Payload::SolidPair(pair));
            self.entries[next as usize] = ErasedCommand::ready(&mut self.prepared, Payload::Empty);
            self.metadata[next as usize].coverage = Coverage::None;
        }
        self.get(id)
    }

    #[inline(always)]
    pub fn get(&self, id: CommandId) -> &Payload {
        let input = &self.entries[id as usize];
        if input.draw.is_some() {
            &Payload::Custom
        } else if input.is_pending() {
            &Payload::Pending
        } else {
            // safety: entries without a draw callback prepare Payload values
            unsafe { input.get(&self.prepared) }
        }
    }

    pub fn vertical_bounds(&self, id: CommandId) -> Range<i32> {
        let bounds = self.metadata[id as usize].bounds;
        bounds.y..bounds.y.saturating_add(bounds.height)
    }

    pub fn horizontal_bounds(&self, id: CommandId) -> Range<i32> {
        let bounds = self.metadata[id as usize].bounds;
        bounds.x..bounds.x.saturating_add(bounds.width)
    }

    pub fn bounds(&self, id: CommandId) -> PhysicalRect {
        self.metadata[id as usize].bounds
    }

    pub fn clip(&self, id: CommandId) -> ClipId {
        self.metadata[id as usize].clip
    }

    pub fn opaque_bounds(&self, id: CommandId, list: &DisplayList) -> Option<PhysicalRect> {
        let metadata = &self.metadata[id as usize];
        match metadata.coverage {
            Coverage::Full => Some(metadata.bounds),
            Coverage::Rect => {
                // safety: rectangular coverage stores the offset of its recorded PhysicalRect
                let bounds = unsafe { list.data().get(self.entries[id as usize].opaque) };
                bounds.intersection(metadata.bounds)
            }
            _ => None,
        }
    }

    pub fn overwrite_span(
        &mut self,
        id: CommandId,
        line: i32,
        list: &DisplayList,
        images: &mut blit_arena::Arena<StoredImage>,
        text: &mut TextRenderer,
    ) -> Option<Range<i32>> {
        let metadata = &self.metadata[id as usize];
        if metadata.coverage == Coverage::None {
            return None;
        }
        if metadata.coverage == Coverage::Rect {
            let bounds = self.opaque_bounds(id, list)?;
            return (line >= bounds.y && line < bounds.y + bounds.height).then_some(bounds.x..bounds.x + bounds.width);
        }
        let partial_image = metadata.coverage == Coverage::PartialImage;
        let area = metadata.bounds;
        let bounds = self.horizontal_bounds(id);
        let span = match self.prepare(id, list, images, text) {
            Payload::Clear => bounds.clone(),
            Payload::Rectangle(rectangle) => rectangle.overwrite_span(line)?,
            Payload::SolidPair(_) => bounds.clone(),
            Payload::Image(image) if partial_image => {
                let texture = images.get(image.image.into())?;
                image.opaque_span(line, area, &texture.data, &texture.alpha_rows)?
            }
            Payload::Image(_) => bounds.clone(),
            Payload::Pending | Payload::Empty | Payload::Text(..) | Payload::Mesh(..) | Payload::Custom => {
                return None;
            }
        };
        let start = span.start.max(bounds.start);
        let end = span.end.min(bounds.end);
        (start < end).then_some(start..end)
    }

    pub fn prepare_meshes(&mut self, list: &DisplayList, damage: &impl DamageTracker) {
        let range = self.range.clone();
        if self.mesh_count == 0 {
            return;
        }
        for id in range.clone() {
            let metadata = Metadata {
                ..self.metadata[id as usize]
            };
            if !damage.intersects(metadata.bounds, metadata.clip) {
                continue;
            }
            let index = match self.get(id) {
                Payload::Mesh(index) => *index as usize,
                _ => continue,
            };
            if (id + 1..range.end).rev().any(|next| {
                self.metadata[next as usize].clip == 0
                    && self
                        .opaque_bounds(next, list)
                        .is_some_and(|bounds| bounds.intersection(metadata.bounds) == Some(metadata.bounds))
            }) {
                self.metadata[id as usize].bounds = PhysicalRect::default();
                continue;
            }
            let mesh = &mut self.meshes[index];
            // safety: mesh recording stores an offset to StoredMesh in this list
            let source = unsafe { list.data().get(mesh.source) };
            let view = source.view(list);
            for (triangle_index, indices) in view.indices.as_chunks::<3>().0.iter().enumerate() {
                let points = indices.map(|vertex_index| {
                    let position = view.vertices[vertex_index as usize].position;
                    [position.x * source.scale, position.y * source.scale]
                });
                let Some(bounds) = crate::render::triangle::bounds(points).intersection(metadata.bounds) else {
                    continue;
                };
                if damage.intersects(bounds, metadata.clip) {
                    mesh.starts.push(mesh.triangles.len() as u32);
                    mesh.triangles
                        .push((bounds, mesh::Triangle::Pending(triangle_index as u32)));
                }
            }
            mesh.starts.sort_unstable_by(|left, right| {
                mesh.triangles[*left as usize]
                    .0
                    .y
                    .cmp(&mesh.triangles[*right as usize].0.y)
                    .then(left.cmp(right))
            });
        }
    }

    pub fn clear(&mut self) {
        self.metadata.clear();
        self.entries.clear();
        self.prepared.clear();
        self.data.clear();
        self.range = 0..0;
        self.gradients.entries.clear();
        self.gradients.stops.clear();
        for mesh in &mut self.meshes[..self.mesh_count] {
            mesh.clear();
        }
        self.mesh_count = 0;
        self.passes.clear();
    }

    pub fn record<T: 'static>(
        &mut self,
        bounds: PhysicalRect,
        clip: ClipId,
        value: T,
        prepare: impl Fn(&mut (), &T, &mut DrawContext<'_, P>) -> Payload + 'static,
    ) -> &mut Metadata {
        let input = ErasedCommand::prepared(&mut self.data, NonNull::<()>::dangling(), value, prepare);
        self.push(input, bounds, clip)
    }

    #[inline(always)]
    pub fn push(&mut self, input: ErasedCommand<P>, bounds: PhysicalRect, clip: ClipId) -> &mut Metadata {
        if input.reads.is_some() {
            self.passes.push(self.len());
        }
        self.entries.push(input);
        self.metadata.push(Metadata {
            bounds,
            clip,
            coverage: Coverage::None,
        });
        self.metadata.last_mut().unwrap()
    }
}

pub enum Payload {
    Pending,
    Empty,
    Clear,
    Rectangle(rectangle::Prepared),
    SolidPair(rectangle::SolidPair),
    Image(image_patch::Prepared),
    Text(PreparedText, PhysicalRect),
    Mesh(u32),
    Custom,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct PreparedText {
    pub glyph_start: u32,
    pub glyph_end: u32,
    pub runs: PreparedRuns,
    pub area: PhysicalRect,
    pub color: Color,
}

pub struct Metadata {
    pub bounds: PhysicalRect,
    pub clip: ClipId,
    pub coverage: Coverage,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    None,
    Full,
    Rounded,
    PartialImage,
    Rect,
}
