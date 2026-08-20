use std::ops::Range;

use blit::{color::Color, geometry::PhysicalRect, paint::GradientStop};

use super::clip::ClipId;
use crate::render::{
    image_patch::Prepared as PreparedImage,
    rectangle::{Gradient as PreparedGradient, Prepared as PreparedRectangle},
};

#[derive(Default)]
pub struct CommandList {
    commands: Vec<StoredCommand>,
    gradient_stops: Vec<GradientStop>,
    overwrites: Vec<usize>,
    partial_opaque: Vec<usize>,
    has_translucent_image: bool,
    pub has_clips: bool,
}

pub enum Payload<'a> {
    Rectangle(&'a PreparedRectangle),
    GradientRectangle(&'a PreparedGradient, &'a [GradientStop]),
    Image(&'a PreparedImage),
    Text(&'a PreparedText),
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct PreparedText {
    pub paragraph: usize,
    pub area: PhysicalRect,
    pub color: Color,
}

impl CommandList {
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn push_rectangle(
        &mut self,
        rectangle: PreparedRectangle,
        bounds: PhysicalRect,
        clip: ClipId,
    ) {
        if clip == 0 && rectangle.overwrites() {
            self.overwrites.push(self.commands.len());
        }
        self.push(StoredPayload::Rectangle(rectangle), bounds, clip)
    }

    pub fn push_gradient_rectangle(
        &mut self,
        rectangle: PreparedGradient,
        stops: &[GradientStop],
        bounds: PhysicalRect,
        clip: ClipId,
    ) -> bool {
        let Ok(start) = u32::try_from(self.gradient_stops.len()) else {
            return false;
        };
        let Ok(len) = u32::try_from(stops.len()) else {
            return false;
        };
        let Some(end) = start.checked_add(len) else {
            return false;
        };
        let index = self.commands.len();
        self.gradient_stops.extend_from_slice(stops);
        self.push(
            StoredPayload::GradientRectangle {
                rectangle,
                stops: start..end,
            },
            bounds,
            clip,
        );
        if clip == 0 && rectangle.overwrites() {
            self.overwrites.push(index);
        }
        true
    }

    pub fn push_image(
        &mut self,
        image: PreparedImage,
        bounds: PhysicalRect,
        clip: ClipId,
        opaque: bool,
        has_opaque_spans: bool,
    ) {
        if clip == 0 && opaque {
            self.overwrites.push(self.commands.len());
        } else if clip == 0 && self.has_translucent_image && has_opaque_spans {
            self.partial_opaque.push(self.commands.len());
        }
        self.has_translucent_image |= !opaque;
        self.push(StoredPayload::Image(image), bounds, clip)
    }

    pub fn push_text(&mut self, text: PreparedText, bounds: PhysicalRect, clip: ClipId) {
        self.push(StoredPayload::Text(text), bounds, clip)
    }

    #[inline]
    pub fn get(&self, index: usize) -> Payload<'_> {
        match &self.commands[index].payload {
            StoredPayload::Rectangle(rectangle) => Payload::Rectangle(rectangle),
            StoredPayload::GradientRectangle { rectangle, stops } => Payload::GradientRectangle(
                rectangle,
                &self.gradient_stops[stops.start as usize..stops.end as usize],
            ),
            StoredPayload::Image(image) => Payload::Image(image),
            StoredPayload::Text(text) => Payload::Text(text),
        }
    }

    pub fn vertical_bounds(&self, index: usize) -> Range<i32> {
        let bounds = self.commands[index].bounds;
        bounds.y..bounds.y.saturating_add(bounds.height)
    }

    pub fn horizontal_bounds(&self, index: usize) -> Range<i32> {
        let bounds = self.commands[index].bounds;
        bounds.x..bounds.x.saturating_add(bounds.width)
    }

    pub fn bounds(&self, index: usize) -> PhysicalRect {
        self.commands[index].bounds
    }

    pub fn clip(&self, index: usize) -> ClipId {
        self.commands[index].clip
    }

    pub fn overwrite_offsets(&self) -> &[usize] {
        &self.overwrites
    }

    pub fn partial_opaque_offsets(&self) -> &[usize] {
        &self.partial_opaque
    }

    pub fn overwrite_span(&self, index: usize, line: i32) -> Option<Range<i32>> {
        let bounds = self.horizontal_bounds(index);
        let span = match self.get(index) {
            Payload::Rectangle(rectangle) => rectangle.overwrite_span(line)?,
            Payload::Image(_) => bounds.clone(),
            Payload::GradientRectangle(rectangle, _) => rectangle.overwrite_span(line)?,
            Payload::Text(_) => return None,
        };
        let start = span.start.max(bounds.start);
        let end = span.end.min(bounds.end);
        (start < end).then_some(start..end)
    }

    pub fn offsets(&self) -> Range<usize> {
        0..self.commands.len()
    }

    pub fn clear(&mut self) {
        self.commands.clear();
        self.gradient_stops.clear();
        self.overwrites.clear();
        self.partial_opaque.clear();
        self.has_translucent_image = false;
        self.has_clips = false;
    }

    fn push(&mut self, payload: StoredPayload, bounds: PhysicalRect, clip: ClipId) {
        self.has_clips |= clip != 0;
        self.commands.push(StoredCommand {
            bounds,
            clip,
            payload,
        });
    }
}

struct StoredCommand {
    bounds: PhysicalRect,
    clip: ClipId,
    payload: StoredPayload,
}

enum StoredPayload {
    Rectangle(PreparedRectangle),
    GradientRectangle {
        rectangle: PreparedGradient,
        stops: Range<u32>,
    },
    Image(PreparedImage),
    Text(PreparedText),
}
