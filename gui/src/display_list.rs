use std::{any::TypeId, hash::Hash, ptr::NonNull};

use blit::{LogicalRect, PhysicalRect, internal::hash::FxHasher};

use crate::{
    arena::{Arena, Writer},
    style::BorderRadius,
};

/// owned drawing inputs and their static backend operations
#[derive(Default)]
pub struct DisplayList {
    commands: Vec<Command>,
    clips: Vec<Clip>,
    data: Arena,
}

pub struct Command {
    pub bounds: PhysicalRect,
    pub clip: u32,
    value: u32,
    ops: &'static Ops,
}

/// shared comparison operations and backend dispatch for one input type
pub struct Ops {
    pub kind: TypeId,
    pub equal: unsafe fn(&Arena, u32, &Arena, u32) -> bool,
    pub hash: unsafe fn(&Arena, u32, &mut FxHasher),
    pub dispatch: unsafe fn(&Command, &Arena, NonNull<()>),
}

#[derive(Clone, Copy)]
pub struct Clip {
    pub parent: u32,
    pub area: LogicalRect,
    pub radius: BorderRadius,
}

impl DisplayList {
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    pub fn data(&self) -> &Arena {
        &self.data
    }

    pub fn writer(&mut self) -> Writer<'_> {
        self.data.writer()
    }

    /// # Safety
    /// the operations must match T and its backend interpretation
    /// matching kinds must have the same representation and comparison semantics
    /// all referenced arena data must belong to this list
    pub unsafe fn push<T: 'static>(&mut self, bounds: PhysicalRect, clip: u32, value: T, ops: &'static Ops) {
        assert!(clip as usize <= self.clips.len(), "invalid paint clip");
        self.commands.push(Command {
            bounds,
            clip,
            value: self.data.push_erased(value),
            ops,
        });
    }

    pub fn push_clip(&mut self, clip: Clip) -> u32 {
        assert!(clip.parent as usize <= self.clips.len(), "invalid parent clip");
        let index = u32::try_from(self.clips.len()).expect("too many paint clips");
        let index = index.checked_add(1).expect("too many paint clips");
        self.clips.push(clip);
        index
    }

    pub fn hash(&self, index: usize, state: &mut FxHasher) {
        let command = &self.commands[index];
        let operations = command.ops;
        operations.kind.hash(state);
        // safety: insertion pairs the operations with their recorded data
        unsafe { (operations.hash)(&self.data, command.value, state) };
    }

    pub fn equivalent(&self, index: usize, other: &Self, other_index: usize) -> bool {
        let left = &self.commands[index];
        let right = &other.commands[other_index];
        let operations = left.ops;
        if left.bounds != right.bounds || operations.kind != right.ops.kind {
            return false;
        }
        let mut clip = left.clip;
        let mut other_clip = right.clip;
        loop {
            match (clip, other_clip) {
                (0, 0) => break,
                (0, _) | (_, 0) => return false,
                _ => {}
            }
            let left = &self.clips[clip as usize - 1];
            let right = &other.clips[other_clip as usize - 1];
            if left.area != right.area || left.radius != right.radius {
                return false;
            }
            clip = left.parent;
            other_clip = right.parent;
        }
        // safety: insertion pairs matching kinds with the same representation
        unsafe { (operations.equal)(&self.data, left.value, &other.data, right.value) }
    }

    pub fn clear(&mut self) {
        self.commands.clear();
        self.clips.clear();
        self.data.clear();
    }
}

impl Command {
    /// # Safety
    /// data must belong to this command's list and context must match its backend
    #[inline]
    pub unsafe fn dispatch(&self, data: &Arena, context: NonNull<()>) {
        // safety: the caller provides this command's data and backend context
        unsafe { (self.ops.dispatch)(self, data, context) };
    }

    pub fn offset(&self) -> u32 {
        self.value
    }
}
