use std::{ops::Range, ptr::NonNull};

use blit::{
    PhysicalRect, ScratchBuffer,
    internal::bump::{Bump, Offset},
};
use blit_gui::display_list::DisplayList;

use super::clip::{ClipId, ClipStack};
use crate::{
    DamageTracker, Pixel, RenderContext, StoredImage,
    damage::region::Region,
    raster::{
        fill::{Gradients, PreparedFill},
        image_patch, mesh, rectangle,
    },
    render::{ErasedCommand, RenderTarget},
    text::{PreparedText, TextRenderer},
};

pub type CommandId = u32;

pub struct RasterCommands<P> {
    pub scale: f32,
    /// drawable buffer extent for this frame
    pub screen: PhysicalRect,
    pub damage_bounds: PhysicalRect,
    /// display list command being dispatched, set by the dispatch loop in `Renderer::render`
    /// entries pushed during that dispatch record it as their source
    pub source: u32,
    pub metadata: Vec<Metadata>,
    pub entries: Vec<ErasedCommand<P>>,
    pub prepared: Bump,
    pub scratch: ScratchBuffer,
    pub range: Range<CommandId>,
    pub gradients: Gradients,
    pub meshes: Vec<mesh::Prepared>,
    pub mesh_count: usize,
    opaque: Region,
}

impl<P> Default for RasterCommands<P> {
    fn default() -> Self {
        Self {
            scale: 1.0,
            screen: PhysicalRect::default(),
            damage_bounds: PhysicalRect::default(),
            source: 0,
            metadata: Vec::new(),
            entries: Vec::new(),
            prepared: Bump::default(),
            scratch: ScratchBuffer::default(),
            range: 0..0,
            gradients: Gradients::default(),
            meshes: Vec::new(),
            mesh_count: 0,
            opaque: Region::default(),
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
        clips: &ClipStack,
        images: &mut blit_arena::Arena<StoredImage>,
        text: &mut TextRenderer,
    ) -> &Payload {
        let command = &mut self.metadata[id as usize];
        let input = &mut self.entries[id as usize];
        if input.render.is_some() {
            return &Payload::Custom;
        }
        if !input.is_pending() {
            // safety: entries without a draw callback prepare Payload values
            return unsafe { input.get(&self.prepared) };
        }
        input.prepare(
            &mut self.prepared,
            &mut RenderContext {
                scale: self.scale,
                area: list.commands()[command.source as usize].area,
                bounds: command.bounds,
                clips,
                clip: command.clip,
                screen: self.screen,
                damage_bounds: self.damage_bounds,
                list,
                text,
                gradients: &mut self.gradients,
                images,
                source: None,
                scratch: &self.scratch,
                target: RenderTarget::Prepare,
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
            && let Payload::Rectangle(second) = self.prepare(next, list, clips, images, text)
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
        if input.render.is_some() {
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

    pub fn opaque_span(
        &mut self,
        id: CommandId,
        line: i32,
        list: &DisplayList,
        clips: &ClipStack,
        images: &mut blit_arena::Arena<StoredImage>,
        text: &mut TextRenderer,
    ) -> Option<Range<i32>> {
        let metadata = &self.metadata[id as usize];
        if metadata.coverage == Coverage::None {
            return None;
        }
        if let Some(bounds) = self.opaque_bounds(id, list) {
            return (line >= bounds.y && line < bounds.y.saturating_add(bounds.height))
                .then_some(bounds.x..bounds.x.saturating_add(bounds.width));
        }
        let area = metadata.bounds;
        let bounds = self.horizontal_bounds(id);
        let span = match self.prepare(id, list, clips, images, text) {
            Payload::Rectangle(rectangle) => rectangle.opaque_span(line)?,
            Payload::Image(image) => {
                let texture = images.get(image.image.into())?;
                image.opaque_span(line, area, &texture.data, &texture.alpha_rows)?
            }
            _ => return None,
        };
        let start = span.start.max(bounds.start);
        let end = span.end.min(bounds.end);
        (start < end).then_some(start..end)
    }

    /// culls covered commands and schedules surviving mesh triangles within one render stage
    pub fn cull(
        &mut self,
        list: &DisplayList,
        clips: &ClipStack,
        damage: &impl DamageTracker,
        screen: PhysicalRect,
        shift: u32,
        ranges: &mut Vec<Range<usize>>,
    ) {
        let mut damage_bounds: Option<PhysicalRect> = None;
        let mut line = screen.y;
        while line < screen.y + screen.height {
            let end = damage.line_ranges(line, screen, ranges);
            for range in ranges.iter() {
                let bounds = PhysicalRect::new(range.start as i32, line, range.len() as i32, end - line);
                damage_bounds = Some(damage_bounds.map_or(bounds, |previous| previous.union(bounds)));
            }
            line = end;
        }
        self.damage_bounds = damage_bounds.unwrap_or_default();
        self.opaque.reset(screen, shift);
        let size = 1usize << shift;
        for id in self.range.clone().rev() {
            let metadata = &self.metadata[id as usize];
            if !damage.intersects(metadata.bounds, metadata.clip) {
                continue;
            }
            let mut bounds = metadata.bounds.intersection(screen);
            let mut opaque = self.opaque_bounds(id, list).and_then(|area| area.intersection(screen));
            if metadata.clip != 0 {
                let clip = &clips.nodes[metadata.clip as usize - 1];
                bounds = bounds.and_then(|bounds| bounds.intersection(clip.bounds));
                opaque = if clip.rectangular {
                    opaque.and_then(|bounds| bounds.intersection(clip.bounds))
                } else {
                    None
                };
            }
            let Some(bounds) = bounds.filter(|bounds| !self.opaque.contains(*bounds)) else {
                self.metadata[id as usize].bounds = PhysicalRect::default();
                continue;
            };
            if let Some(opaque) = opaque
                && id > self.range.start
            {
                // only whole cells are opaque and screen edge cells may be smaller
                let left = (opaque.x as usize).div_ceil(size) * size;
                let top = (opaque.y as usize).div_ceil(size) * size;
                let right = (opaque.x + opaque.width) as usize;
                let bottom = (opaque.y + opaque.height) as usize;
                let right = if right == screen.width as usize {
                    right
                } else {
                    right & !(size - 1)
                };
                let bottom = if bottom == screen.height as usize {
                    bottom
                } else {
                    bottom & !(size - 1)
                };
                if left < right && top < bottom {
                    self.opaque.add(PhysicalRect::new(
                        left as i32,
                        top as i32,
                        (right - left) as i32,
                        (bottom - top) as i32,
                    ));
                    if self.opaque.contains(screen) {
                        for metadata in &mut self.metadata[self.range.start as usize..id as usize] {
                            metadata.bounds = PhysicalRect::default();
                        }
                        break;
                    }
                }
            }
            let Payload::Mesh(index) = self.get(id) else {
                continue;
            };
            let index = *index as usize;
            let clip = self.metadata[id as usize].clip;
            let mesh = &mut self.meshes[index];
            // safety: mesh recording stores an offset to blit_gui::mesh::Stored in this list
            let source = unsafe { list.data().get(mesh.source) };
            let view = source.view(list);
            for (triangle_index, indices) in view.indices.as_chunks::<3>().0.iter().enumerate() {
                let points = indices.map(|vertex_index| {
                    let position = view.vertices[vertex_index as usize].position;
                    [position.x * self.scale, position.y * self.scale]
                });
                let Some(bounds) = crate::raster::triangle::bounds(points).intersection(bounds) else {
                    continue;
                };
                if damage.intersects(bounds, clip) && !self.opaque.contains(bounds) {
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
            if mesh.triangles.is_empty() {
                self.metadata[id as usize].bounds = PhysicalRect::default();
            }
        }
    }

    pub fn clear(&mut self) {
        self.scratch.prepare();
        self.metadata.clear();
        self.entries.clear();
        self.prepared.clear();
        self.range = 0..0;
        self.gradients.entries.clear();
        self.gradients.stops.clear();
        for mesh in &mut self.meshes[..self.mesh_count] {
            mesh.clear();
        }
        self.mesh_count = 0;
    }

    pub fn record(
        &mut self,
        bounds: PhysicalRect,
        clip: ClipId,
        value: u32,
        prepare: fn(NonNull<()>, u32, &mut Bump, &mut RenderContext<'_, P>) -> u32,
    ) -> &mut Metadata {
        let input = ErasedCommand {
            renderer: NonNull::dangling(),
            value,
            opaque: Offset::new(0),
            render: None,
            reads: None,
            prepare: Some(prepare),
        };
        self.push(input, bounds, clip)
    }

    #[inline(always)]
    pub fn push(&mut self, input: ErasedCommand<P>, bounds: PhysicalRect, clip: ClipId) -> &mut Metadata {
        self.entries.push(input);
        self.metadata.push(Metadata {
            bounds,
            clip,
            source: self.source,
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

pub struct Metadata {
    pub bounds: PhysicalRect,
    pub clip: ClipId,
    /// display list command that produced this entry
    pub source: u32,
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
