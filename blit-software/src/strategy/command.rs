use std::mem::MaybeUninit;

use blit::{Color, PhysicalRect, widgets::GradientStop};

use crate::{
    image::Prepared as PreparedImage,
    rectangle::{Gradient as PreparedGradient, Prepared as PreparedRectangle},
};

use super::clip::ClipId;

const RECTANGLE: u8 = 0;
const GRADIENT_RECTANGLE: u8 = 1;
const IMAGE: u8 = 2;
const TEXT: u8 = 3;

/// packed records avoid sizing every command for the largest enum variant
#[derive(Default)]
pub struct CommandList {
    words: Vec<Word>,
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
        self.words.is_empty()
    }

    pub fn push_rectangle(
        &mut self,
        rectangle: PreparedRectangle,
        bounds: PhysicalRect,
        clip: ClipId,
    ) {
        self.push(RECTANGLE, rectangle, bounds, clip)
    }

    pub fn push_gradient_rectangle(
        &mut self,
        rectangle: PreparedGradient,
        stops: &[GradientStop],
        bounds: PhysicalRect,
        clip: ClipId,
    ) -> bool {
        let Ok(stops_len) = stops.len().try_into() else {
            return false;
        };
        self.push_record(
            GRADIENT_RECTANGLE,
            PreparedGradientRectangle {
                rectangle,
                stops_len,
            },
            stops,
            bounds,
            clip,
        )
    }

    pub fn push_image(&mut self, image: PreparedImage, bounds: PhysicalRect, clip: ClipId) {
        self.push(IMAGE, image, bounds, clip)
    }

    pub fn push_text(&mut self, text: PreparedText, bounds: PhysicalRect, clip: ClipId) {
        self.push(TEXT, text, bounds, clip)
    }

    #[inline]
    pub fn get(&self, offset: usize) -> Payload<'_> {
        let record = unsafe { self.words.as_ptr().add(offset).cast::<u8>() };
        let header = self.header(offset);
        let payload = match header.kind {
            RECTANGLE => Payload::Rectangle(unsafe {
                &*record
                    .add(payload_offset::<PreparedRectangle>())
                    .cast::<PreparedRectangle>()
            }),
            GRADIENT_RECTANGLE => {
                let command = unsafe {
                    &*record
                        .add(payload_offset::<PreparedGradientRectangle>())
                        .cast::<PreparedGradientRectangle>()
                };
                let stops_offset = (payload_offset::<PreparedGradientRectangle>()
                    + size_of::<PreparedGradientRectangle>())
                .next_multiple_of(align_of::<GradientStop>());
                Payload::GradientRectangle(&command.rectangle, unsafe {
                    std::slice::from_raw_parts(
                        record.add(stops_offset).cast::<GradientStop>(),
                        command.stops_len as usize,
                    )
                })
            }
            IMAGE => Payload::Image(unsafe {
                &*record
                    .add(payload_offset::<PreparedImage>())
                    .cast::<PreparedImage>()
            }),
            TEXT => Payload::Text(unsafe {
                &*record
                    .add(payload_offset::<PreparedText>())
                    .cast::<PreparedText>()
            }),
            _ => unreachable!(),
        };
        payload
    }

    pub fn vertical_bounds(&self, offset: usize) -> std::ops::Range<i32> {
        let header = self.header(offset);
        header.top..header.bottom
    }

    pub fn horizontal_bounds(&self, offset: usize) -> std::ops::Range<i32> {
        let header = self.header(offset);
        header.left..header.right
    }

    pub fn clip(&self, offset: usize) -> ClipId {
        self.header(offset).clip
    }

    pub fn offsets(&self) -> Offsets<'_> {
        Offsets {
            commands: self,
            offset: 0,
        }
    }

    pub fn clear(&mut self) {
        self.words.clear();
        self.has_clips = false;
    }

    fn push<T: Copy>(&mut self, kind: u8, payload: T, bounds: PhysicalRect, clip: ClipId) {
        assert!(self.push_record(kind, payload, &[], bounds, clip));
    }

    fn push_record<T: Copy>(
        &mut self,
        kind: u8,
        payload: T,
        stops: &[GradientStop],
        bounds: PhysicalRect,
        clip: ClipId,
    ) -> bool {
        assert!(align_of::<T>() <= align_of::<Word>());
        let payload_offset = payload_offset::<T>();
        let stops_offset =
            (payload_offset + size_of::<T>()).next_multiple_of(align_of::<GradientStop>());
        let bytes = if stops.is_empty() {
            payload_offset + size_of::<T>()
        } else {
            stops_offset + size_of_val(stops)
        };
        let record_words = bytes.div_ceil(size_of::<Word>());
        let Ok(record_words) = record_words.try_into() else {
            return false;
        };
        self.has_clips |= clip != 0;
        let offset = self.words.len();
        self.words.resize_with(offset + record_words as usize, || {
            Word(MaybeUninit::uninit())
        });
        let record = unsafe { self.words.as_mut_ptr().add(offset).cast::<u8>() };
        unsafe {
            record.cast::<Header>().write(Header {
                top: bounds.y,
                bottom: bounds.y.saturating_add(bounds.height),
                left: bounds.x,
                right: bounds.x.saturating_add(bounds.width),
                record_words,
                kind,
                clip,
            });
            record.add(payload_offset).cast::<T>().write(payload);
            if !stops.is_empty() {
                record
                    .add(stops_offset)
                    .cast::<GradientStop>()
                    .copy_from_nonoverlapping(stops.as_ptr(), stops.len());
            }
        }
        true
    }

    fn record_words(&self, offset: usize) -> usize {
        self.header(offset).record_words as usize
    }

    fn header(&self, offset: usize) -> &Header {
        assert!(offset < self.words.len());
        unsafe { &*self.words.as_ptr().add(offset).cast::<Header>() }
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct PreparedGradientRectangle {
    rectangle: PreparedGradient,
    stops_len: u32,
}

fn payload_offset<T>() -> usize {
    size_of::<Header>().next_multiple_of(align_of::<T>())
}

pub struct Offsets<'a> {
    commands: &'a CommandList,
    offset: usize,
}

impl Iterator for Offsets<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset == self.commands.words.len() {
            return None;
        }
        let offset = self.offset;
        self.offset += self.commands.record_words(offset);
        Some(offset)
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Header {
    top: i32,
    bottom: i32,
    left: i32,
    right: i32,
    record_words: u8,
    kind: u8,
    clip: u16,
}

#[repr(C, align(8))]
struct Word(MaybeUninit<[u8; 8]>);
