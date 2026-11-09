use std::{ops::Range, ptr::NonNull};

use blit::{PhysicalRect, Scale2};
use blit_gui::{color::Color, paint::MeshVertex};

use super::clip::ClipId;
use crate::{
    DamageTracker, DrawContext, Pixel, StoredImage,
    arena::{Arena, Slice},
    paint::{DrawTarget, ErasedCommand},
    render::{
        fill::Gradients,
        image_patch::Prepared as PreparedImage,
        rectangle::{Prepared as PreparedRectangle, SolidPair as PreparedSolidPair},
        triangle::Prepared as PreparedTriangle,
    },
    text::{PreparedRuns, TextRenderer},
};

pub type CommandId = u32;

pub struct CommandList<P> {
    pub metadata: Vec<Metadata>,
    pub entries: Vec<ErasedCommand<P>>,
    pub data: Arena,
    pub prepared: Arena,
    pub range: Option<Range<CommandId>>,
    pub gradients: Gradients,
    pub has_partial_opaque: bool,
    pub has_clips: bool,
    pub has_meshes: bool,
    pub has_passes: bool,
}

impl<P> Default for CommandList<P> {
    fn default() -> Self {
        Self {
            metadata: Vec::new(),
            entries: Vec::new(),
            data: Arena::default(),
            prepared: Arena::default(),
            range: None,
            gradients: Gradients::default(),
            has_partial_opaque: false,
            has_clips: false,
            has_meshes: false,
            has_passes: false,
        }
    }
}

pub enum Payload {
    Pending,
    Empty,
    Clear,
    Rectangle(PreparedRectangle),
    SolidPair(PreparedSolidPair),
    Image(PreparedImage),
    Text(PreparedText, PhysicalRect),
    Triangle(PreparedTriangle),
    Mesh(Slice<MeshVertex>, Slice<u32>, f32),
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

impl<P: Pixel> CommandList<P> {
    pub fn len(&self) -> CommandId {
        CommandId::try_from(self.metadata.len()).expect("too many cpu commands in one frame")
    }

    pub fn prepare(&mut self, id: CommandId, images: &mut blit_arena::Arena<StoredImage>, text: &mut TextRenderer) {
        let command = &mut self.metadata[id as usize];
        let input = &mut self.entries[id as usize];
        if input.draw.is_some() {
            return;
        }
        if !input.is_pending() {
            return;
        }
        input.prepare(
            &self.data,
            &mut self.prepared,
            &mut DrawContext {
                bounds: command.bounds,
                data: &self.data,
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
            return;
        };
        let first = *first;
        let next = id + 1;
        if first.overwrites()
            && first.radii.is_zero()
            && first.border_width == 0
            && self.clip(id) == 0
            && next < self.offsets().end
            && self.clip(next) == 0
            && self.bounds(id) == self.bounds(next)
            && !matches!(self.get(next), Payload::Custom)
        {
            self.prepare(next, images, text);
            if let Payload::Rectangle(second) = self.get(next)
                && let Some(pair) = first.fuse(second)
            {
                self.entries[id as usize] =
                    ErasedCommand::ready(NonNull::<()>::dangling(), &mut self.prepared, Payload::SolidPair(pair));
                self.entries[next as usize] =
                    ErasedCommand::ready(NonNull::<()>::dangling(), &mut self.prepared, Payload::Empty);
                self.metadata[next as usize].overwrites = false;
                self.metadata[next as usize].partial_opaque = false;
            }
        }
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

    pub fn overwrites(&self, id: CommandId) -> bool {
        self.metadata[id as usize].overwrites
    }

    pub fn has_partial_opaque(&self) -> bool {
        self.has_partial_opaque
    }

    pub fn partial_opaque(&self, id: CommandId) -> bool {
        self.metadata[id as usize].partial_opaque
    }

    pub fn overwrite_span(
        &mut self,
        id: CommandId,
        line: i32,
        images: &mut blit_arena::Arena<StoredImage>,
        text: &mut TextRenderer,
    ) -> Option<Range<i32>> {
        if matches!(self.get(id), Payload::Pending) {
            self.prepare(id, images, text);
        }
        let bounds = self.horizontal_bounds(id);
        let span = match self.get(id) {
            Payload::Clear => bounds.clone(),
            Payload::Rectangle(rectangle) => rectangle.overwrite_span(line)?,
            Payload::SolidPair(_) => bounds.clone(),
            Payload::Image(_) => bounds.clone(),
            Payload::Pending
            | Payload::Empty
            | Payload::Text(..)
            | Payload::Triangle(_)
            | Payload::Mesh(..)
            | Payload::Custom => {
                return None;
            }
        };
        let start = span.start.max(bounds.start);
        let end = span.end.min(bounds.end);
        (start < end).then_some(start..end)
    }

    pub fn offsets(&self) -> Range<CommandId> {
        self.range.clone().unwrap_or_else(|| 0..self.len())
    }

    pub fn expand_meshes(
        &mut self,
        damage: &impl DamageTracker,
        images: &mut blit_arena::Arena<StoredImage>,
        text: &mut TextRenderer,
    ) {
        let range = self.offsets();
        if !self.has_meshes || !range.clone().any(|id| matches!(self.get(id), Payload::Mesh(..))) {
            return;
        }
        let start = self.len();
        for id in range.clone() {
            let metadata = Metadata {
                ..self.metadata[id as usize]
            };
            if !damage.intersects(metadata.bounds, metadata.clip) {
                continue;
            }
            let (vertices, indices, scale) = match self.get(id) {
                Payload::Mesh(vertices, indices, scale) => (*vertices, *indices, *scale),
                _ => {
                    self.entries.push(self.entries[id as usize]);
                    self.metadata.push(metadata);
                    continue;
                }
            };
            if (id + 1..range.end).rev().any(|next| {
                if !self.overwrites(next)
                    || self.clip(next) != 0
                    || self.bounds(next).intersection(metadata.bounds) != Some(metadata.bounds)
                {
                    return false;
                }
                self.prepare(next, images, text);
                match self.get(next) {
                    Payload::Clear | Payload::SolidPair(_) | Payload::Image(_) => true,
                    Payload::Rectangle(rectangle) => rectangle.radii.is_zero() && rectangle.overwrites(),
                    _ => false,
                }
            }) {
                continue;
            }
            for triangle in 0..self.data.slice(indices).len() / 3 {
                let indices: [u32; 3] = self.data.slice(indices)[triangle * 3..][..3].try_into().unwrap();
                let points = indices.map(|index| {
                    let vertex = self.data.slice(vertices)[index as usize];
                    [vertex.position.x * scale, vertex.position.y * scale]
                });
                let Some(bounds) = crate::render::triangle::bounds(points).intersection(metadata.bounds) else {
                    continue;
                };
                if damage.intersects(bounds, metadata.clip) {
                    self.record(
                        bounds,
                        metadata.clip,
                        (vertices, indices, scale),
                        |_, (vertices, indices, scale), cx| {
                            let vertices = cx.data.slice(*vertices);
                            let vertices = indices.map(|index| vertices[index as usize]);
                            PreparedTriangle::new(vertices, Scale2::uniform(*scale), cx.bounds)
                                .map(|(triangle, _)| Payload::Triangle(triangle))
                                .unwrap_or(Payload::Empty)
                        },
                    );
                }
            }
        }
        self.range = Some(start..self.len());
    }

    pub fn clear(&mut self) {
        self.metadata.clear();
        self.entries.clear();
        self.prepared.clear();
        self.data.clear();
        self.range = None;
        self.gradients.entries.clear();
        self.gradients.stops.clear();
        self.has_partial_opaque = false;
        self.has_clips = false;
        self.has_meshes = false;
        self.has_passes = false;
    }
}

impl<P: Pixel> CommandList<P> {
    pub fn record<T: 'static>(
        &mut self,
        bounds: PhysicalRect,
        clip: ClipId,
        value: T,
        prepare: fn(&mut (), &T, &mut DrawContext<'_, P>) -> Payload,
    ) -> &mut Metadata {
        let input = ErasedCommand::prepared(&mut self.data, NonNull::<()>::dangling(), value, prepare);
        self.push(input, bounds, clip)
    }

    pub fn push(&mut self, input: ErasedCommand<P>, bounds: PhysicalRect, clip: ClipId) -> &mut Metadata {
        self.has_clips |= clip != 0;
        self.has_passes |= input.reads.is_some();
        self.entries.push(input);
        self.metadata.push(Metadata {
            bounds,
            clip,
            overwrites: false,
            partial_opaque: false,
        });
        self.metadata.last_mut().unwrap()
    }
}

pub struct Metadata {
    pub bounds: PhysicalRect,
    pub clip: ClipId,
    pub overwrites: bool,
    pub partial_opaque: bool,
}
