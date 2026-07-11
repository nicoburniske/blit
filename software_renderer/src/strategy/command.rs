use std::{mem::MaybeUninit, slice};

use bullseye::{Color, PhysicalRect};

use crate::{image::Prepared as PreparedImage, rectangle::Prepared as PreparedRectangle};

const RECTANGLE: u8 = 0;
const IMAGE: u8 = 1;
const TEXT: u8 = 2;

/// packed records avoid sizing every command for the largest enum variant
#[derive(Default)]
pub struct CommandList {
    words: Vec<Word>,
}

pub struct Command<'a> {
    pub clips: &'a [PhysicalRect],
    pub payload: Payload<'a>,
}

pub enum Payload<'a> {
    Rectangle(&'a PreparedRectangle),
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

    pub fn push_rectangle(&mut self, rectangle: PreparedRectangle, clips: &[PhysicalRect]) {
        self.push(RECTANGLE, rectangle, clips)
    }

    pub fn push_image(&mut self, image: PreparedImage, clips: &[PhysicalRect]) {
        self.push(IMAGE, image, clips)
    }

    pub fn push_text(&mut self, text: PreparedText, clips: &[PhysicalRect]) {
        self.push(TEXT, text, clips)
    }

    pub fn get(&self, offset: usize) -> Command<'_> {
        let record = unsafe { self.words.as_ptr().add(offset).cast::<u8>() };
        let header = self.header(offset);
        let clips = unsafe {
            slice::from_raw_parts(
                record.add(clips_offset()).cast::<PhysicalRect>(),
                header.clip_count as usize,
            )
        };
        let payload = match header.kind {
            RECTANGLE => Payload::Rectangle(unsafe {
                &*record
                    .add(payload_offset::<PreparedRectangle>(clips.len()))
                    .cast::<PreparedRectangle>()
            }),
            IMAGE => Payload::Image(unsafe {
                &*record
                    .add(payload_offset::<PreparedImage>(clips.len()))
                    .cast::<PreparedImage>()
            }),
            TEXT => Payload::Text(unsafe {
                &*record
                    .add(payload_offset::<PreparedText>(clips.len()))
                    .cast::<PreparedText>()
            }),
            _ => unreachable!(),
        };
        Command { clips, payload }
    }

    pub fn vertical_bounds(&self, offset: usize) -> std::ops::Range<i32> {
        let header = self.header(offset);
        header.top..header.bottom
    }

    pub fn offsets(&self) -> Offsets<'_> {
        Offsets {
            commands: self,
            offset: 0,
        }
    }

    pub fn clear(&mut self) {
        self.words.clear()
    }

    fn push<T: Copy>(&mut self, kind: u8, payload: T, clips: &[PhysicalRect]) {
        assert!(!clips.is_empty());
        assert!(clips.len() <= 8);
        assert!(align_of::<T>() <= align_of::<Word>());
        let payload_offset = payload_offset::<T>(clips.len());
        let bytes = payload_offset + size_of::<T>();
        let record_words = bytes.div_ceil(size_of::<Word>());
        let offset = self.words.len();
        self.words
            .resize_with(offset + record_words, Word::uninitialized);
        let record = unsafe { self.words.as_mut_ptr().add(offset).cast::<u8>() };
        let top = clips.iter().map(|clip| clip.y).min().unwrap();
        let bottom = clips
            .iter()
            .map(|clip| clip.y.saturating_add(clip.height))
            .max()
            .unwrap();
        unsafe {
            record.cast::<Header>().write(Header {
                top,
                bottom,
                record_words: record_words.try_into().unwrap(),
                clip_count: clips.len().try_into().unwrap(),
                kind,
                padding: 0,
            });
            let stored_clips = record.add(clips_offset()).cast::<PhysicalRect>();
            stored_clips.copy_from_nonoverlapping(clips.as_ptr(), clips.len());
            record.add(payload_offset).cast::<T>().write(payload);
        }
    }

    fn record_words(&self, offset: usize) -> usize {
        self.header(offset).record_words as usize
    }

    fn header(&self, offset: usize) -> &Header {
        assert!(offset < self.words.len());
        unsafe { &*self.words.as_ptr().add(offset).cast::<Header>() }
    }
}

fn clips_offset() -> usize {
    size_of::<Header>().next_multiple_of(align_of::<PhysicalRect>())
}

fn payload_offset<T>(clip_count: usize) -> usize {
    (clips_offset() + clip_count * size_of::<PhysicalRect>()).next_multiple_of(align_of::<T>())
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
    record_words: u16,
    clip_count: u8,
    kind: u8,
    padding: u32,
}

#[repr(C, align(8))]
struct Word(MaybeUninit<[u8; 8]>);

impl Word {
    fn uninitialized() -> Self {
        Self(MaybeUninit::uninit())
    }
}
