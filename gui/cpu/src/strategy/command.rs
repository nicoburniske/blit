use std::ops::Range;

use blit::PhysicalRect;
use blit_gui::color::Color;

use super::clip::ClipId;
use crate::{
    arena::Arena,
    paint::Callback,
    render::{
        fill::Gradients,
        image_patch::Prepared as PreparedImage,
        rectangle::{Prepared as PreparedRectangle, SolidPair as PreparedSolidPair},
        triangle::Prepared as PreparedTriangle,
    },
    text::PreparedRuns,
};

pub type CommandId = u32;

pub struct CommandList<P> {
    commands: Vec<StoredCommand>,
    pub callbacks: Vec<Callback<P>>,
    pub data: Arena,
    pub prepared: Arena,
    pub range: Option<Range<CommandId>>,
    pub gradients: Gradients,
    has_partial_opaque: bool,
    pub has_clips: bool,
}

impl<P> Default for CommandList<P> {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
            callbacks: Vec::new(),
            data: Arena::default(),
            prepared: Arena::default(),
            range: None,
            gradients: Gradients::default(),
            has_partial_opaque: false,
            has_clips: false,
        }
    }
}

pub enum Payload<'a> {
    Clear,
    Rectangle(&'a PreparedRectangle, &'a Gradients),
    SolidPair(&'a PreparedSolidPair),
    Image(&'a PreparedImage),
    Text(&'a PreparedText),
    Triangle(&'a PreparedTriangle),
    Callback(u32),
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

impl<P> CommandList<P> {
    pub fn len(&self) -> CommandId {
        CommandId::try_from(self.commands.len()).expect("too many cpu commands in one frame")
    }

    pub fn push_callback(&mut self, call: Callback<P>, bounds: PhysicalRect, clip: ClipId) {
        let index = u32::try_from(self.callbacks.len()).expect("too many cpu callbacks in one frame");
        self.callbacks.push(call);
        self.push(StoredPayload::Callback(index), bounds, clip, false, false);
    }

    pub fn push_clear(&mut self, bounds: PhysicalRect) {
        self.push(StoredPayload::Clear, bounds, 0, true, false);
    }

    pub fn push_rectangle(&mut self, rectangle: PreparedRectangle, bounds: PhysicalRect, clip: ClipId) {
        let overwrites = rectangle.overwrites();
        if clip == 0
            && let Some(previous) = self.commands.last_mut()
            && previous.clip == 0
            && previous.bounds == bounds
        {
            if overwrites && rectangle.radii.is_zero() {
                previous.payload = StoredPayload::Rectangle(rectangle);
                previous.overwrites = true;
                previous.partial_opaque = false;
                return;
            }
            if let StoredPayload::Rectangle(first) = &previous.payload
                && let Some(pair) = first.fuse(&rectangle)
            {
                previous.payload = StoredPayload::SolidPair(pair);
                return;
            }
        }
        self.push(StoredPayload::Rectangle(rectangle), bounds, clip, overwrites, false);
    }

    pub fn push_image(
        &mut self,
        image: PreparedImage,
        bounds: PhysicalRect,
        clip: ClipId,
        opaque: bool,
        has_opaque_spans: bool,
    ) {
        let partial_opaque = !opaque && has_opaque_spans;
        self.push(StoredPayload::Image(image), bounds, clip, opaque, partial_opaque);
        self.has_partial_opaque |= partial_opaque;
    }

    pub fn push_text(&mut self, text: PreparedText, bounds: PhysicalRect, clip: ClipId) {
        self.push(StoredPayload::Text(text), bounds, clip, false, false);
    }

    pub fn push_triangle(&mut self, triangle: PreparedTriangle, bounds: PhysicalRect, clip: ClipId) {
        self.push(StoredPayload::Triangle(triangle), bounds, clip, false, false);
    }

    #[inline]
    pub fn get(&self, id: CommandId) -> Payload<'_> {
        match &self.commands[id as usize].payload {
            StoredPayload::Clear => Payload::Clear,
            StoredPayload::Rectangle(rectangle) => Payload::Rectangle(rectangle, &self.gradients),
            StoredPayload::SolidPair(pair) => Payload::SolidPair(pair),
            StoredPayload::Image(image) => Payload::Image(image),
            StoredPayload::Text(text) => Payload::Text(text),
            StoredPayload::Triangle(triangle) => Payload::Triangle(triangle),
            StoredPayload::Callback(index) => Payload::Callback(*index),
        }
    }

    pub fn vertical_bounds(&self, id: CommandId) -> Range<i32> {
        let bounds = self.commands[id as usize].bounds;
        bounds.y..bounds.y.saturating_add(bounds.height)
    }

    pub fn horizontal_bounds(&self, id: CommandId) -> Range<i32> {
        let bounds = self.commands[id as usize].bounds;
        bounds.x..bounds.x.saturating_add(bounds.width)
    }

    pub fn bounds(&self, id: CommandId) -> PhysicalRect {
        self.commands[id as usize].bounds
    }

    pub fn clip(&self, id: CommandId) -> ClipId {
        self.commands[id as usize].clip
    }

    pub fn overwrites(&self, id: CommandId) -> bool {
        self.commands[id as usize].overwrites
    }

    pub fn has_partial_opaque(&self) -> bool {
        self.has_partial_opaque
    }

    pub fn partial_opaque(&self, id: CommandId) -> bool {
        self.commands[id as usize].partial_opaque
    }

    pub fn overwrite_span(&self, id: CommandId, line: i32) -> Option<Range<i32>> {
        let bounds = self.horizontal_bounds(id);
        let span = match self.get(id) {
            Payload::Clear => bounds.clone(),
            Payload::Rectangle(rectangle, _) => rectangle.overwrite_span(line)?,
            Payload::SolidPair(_) => bounds.clone(),
            Payload::Image(_) => bounds.clone(),
            Payload::Text(_) | Payload::Triangle(_) | Payload::Callback(_) => {
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

    pub fn clear(&mut self) {
        self.commands.clear();
        self.callbacks.clear();
        self.prepared.clear();
        self.data.clear();
        self.range = None;
        self.gradients.entries.clear();
        self.gradients.stops.clear();
        self.has_partial_opaque = false;
        self.has_clips = false;
    }
}

impl<P> CommandList<P> {
    fn push(
        &mut self,
        payload: StoredPayload,
        bounds: PhysicalRect,
        clip: ClipId,
        overwrites: bool,
        partial_opaque: bool,
    ) {
        self.has_clips |= clip != 0;
        self.commands.push(StoredCommand {
            bounds,
            clip,
            overwrites,
            partial_opaque,
            payload,
        });
    }
}

struct StoredCommand {
    bounds: PhysicalRect,
    clip: ClipId,
    overwrites: bool,
    partial_opaque: bool,
    payload: StoredPayload,
}

enum StoredPayload {
    Clear,
    Rectangle(PreparedRectangle),
    SolidPair(PreparedSolidPair),
    Image(PreparedImage),
    Text(PreparedText),
    Triangle(PreparedTriangle),
    Callback(u32),
}
