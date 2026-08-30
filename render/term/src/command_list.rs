//! fully resolved terminal commands

use crate::{color::Color, image::ImagePlacement, text::TextRequest};
use blit::{LogicalRect, PhysicalRect};

#[derive(Default)]
pub struct CommandList {
    commands: Vec<StoredCommand>,
    clips: Vec<ClipNode>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    pub bounds: PhysicalRect,
    pub clip: ClipId,
    pub command: Command,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// restores cells to their default value
    Clear,
    Block(Block),
    Image(ImagePlacement),
    Text(TextRequest),
}

blit::builder! {
    /// terminal cell background and optional box-drawing border
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Block {
        new(area: LogicalRect),
        @optional {
            border: Border,
            background: Color,
        },
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Border {
        new(color: Color),
        rounded: bool = false,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ClipId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipNode {
    pub parent: ClipId,
    pub area: LogicalRect,
}

impl CommandList {
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn push_clip(&mut self, parent: ClipId, area: LogicalRect) -> ClipId {
        self.assert_clip(parent);
        let id = u32::try_from(self.clips.len() + 1).expect("too many command list clips");
        self.clips.push(ClipNode { parent, area });
        ClipId(id)
    }

    pub fn clip(&self, id: ClipId) -> Option<&ClipNode> {
        id.0.checked_sub(1)
            .and_then(|index| self.clips.get(index as usize))
    }

    pub fn clips(&self) -> &[ClipNode] {
        &self.clips
    }

    pub fn push_clear(&mut self, bounds: PhysicalRect) {
        self.push(bounds, ClipId::default(), Command::Clear)
    }

    pub fn push_block(&mut self, block: Block, bounds: PhysicalRect, clip: ClipId) {
        self.push(bounds, clip, Command::Block(block))
    }

    pub fn push_image(&mut self, image: ImagePlacement, bounds: PhysicalRect, clip: ClipId) {
        self.push(bounds, clip, Command::Image(image))
    }

    pub fn push_text(&mut self, text: TextRequest, bounds: PhysicalRect, clip: ClipId) {
        self.push(bounds, clip, Command::Text(text))
    }

    pub fn get(&self, index: usize) -> Record {
        let command = self.commands[index];
        Record {
            bounds: command.bounds,
            clip: command.clip,
            command: command.command,
        }
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter {
            list: self,
            front: 0,
            back: self.len(),
        }
    }

    pub fn clear(&mut self) {
        self.commands.clear();
        self.clips.clear();
    }

    pub fn equivalent(&self, index: usize, other: &Self, other_index: usize) -> bool {
        let left = self.commands[index];
        let right = other.commands[other_index];
        left.bounds == right.bounds
            && left.command == right.command
            && self.clips_equal(left.clip, other, right.clip)
    }

    fn push(&mut self, bounds: PhysicalRect, clip: ClipId, command: Command) {
        self.assert_clip(clip);
        self.commands.push(StoredCommand {
            bounds,
            clip,
            command,
        });
    }

    fn clips_equal(&self, mut clip: ClipId, other: &Self, mut other_clip: ClipId) -> bool {
        loop {
            match (clip.0, other_clip.0) {
                (0, 0) => return true,
                (0, _) | (_, 0) => return false,
                _ => {}
            }
            let left = self.clip(clip).unwrap();
            let right = other.clip(other_clip).unwrap();
            if left.area != right.area {
                return false;
            }
            clip = left.parent;
            other_clip = right.parent;
        }
    }

    fn assert_clip(&self, clip: ClipId) {
        assert!(
            clip.0 as usize <= self.clips.len(),
            "invalid command list clip"
        );
    }
}

#[derive(Clone, Copy)]
struct StoredCommand {
    bounds: PhysicalRect,
    clip: ClipId,
    command: Command,
}

pub struct Iter<'a> {
    list: &'a CommandList,
    front: usize,
    back: usize,
}

impl Iterator for Iter<'_> {
    type Item = Record;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let record = self.list.get(self.front);
        self.front += 1;
        Some(record)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl DoubleEndedIterator for Iter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        Some(self.list.get(self.back))
    }
}

impl ExactSizeIterator for Iter<'_> {
    fn len(&self) -> usize {
        self.back - self.front
    }
}
