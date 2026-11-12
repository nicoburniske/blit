use std::{
    any::TypeId,
    hash::{Hash, Hasher},
    mem::offset_of,
    ptr::NonNull,
};

use blit::{
    FrameStore, LogicalRect, PhysicalRect,
    internal::{
        bump::{Bump, Offset},
        hash::FxHasher,
    },
};

use crate::style::BorderRadius;

/// owned drawing inputs and their static backend operations
#[derive(Default)]
pub struct DisplayList {
    commands: Vec<Command>,
    clips: Vec<Clip>,
    data: Bump,
    storage: FrameStore,
}

/// content comparison for recorded drawing inputs
///
/// equal values must hash identically
/// resolve stored slices through their corresponding display lists
pub trait Compare: 'static {
    fn equal(a: &Self, left: &DisplayList, b: &Self, right: &DisplayList) -> bool;
    fn hash(input: &Self, list: &DisplayList, state: &mut impl Hasher);
}

impl<T> Compare for T
where
    T: PartialEq + Hash + 'static,
{
    fn equal(a: &Self, _: &DisplayList, b: &Self, _: &DisplayList) -> bool {
        a == b
    }
    fn hash(input: &Self, _: &DisplayList, state: &mut impl Hasher) {
        Hash::hash(input, state);
    }
}

impl DisplayList {
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    pub fn data(&self) -> &Bump {
        &self.data
    }

    pub fn storage(&self) -> &FrameStore {
        &self.storage
    }

    pub fn storage_mut(&mut self) -> &mut FrameStore {
        &mut self.storage
    }

    /// # Safety
    /// the operations must match T and its backend interpretation
    /// matching kinds must have the same representation and comparison semantics
    /// all referenced storage must belong to this list
    pub unsafe fn push<T: 'static>(&mut self, bounds: PhysicalRect, clip: u32, value: T, ops: &'static Ops) {
        assert!(clip as usize <= self.clips.len(), "invalid paint clip");
        self.commands.push(Command {
            bounds,
            clip,
            value: self.data.push(value).byte(),
            ops,
        });
    }

    /// records owned inputs with typed comparison and backend dispatch
    ///
    /// # Safety
    /// commands must be dispatched with a context matching C
    /// all referenced storage must belong to this list
    pub unsafe fn record<T, F, C>(&mut self, bounds: PhysicalRect, clip: u32, value: T, prepare: F)
    where
        T: Compare,
        F: Fn(&T, u32, &Command, &DisplayList, &mut C) + 'static,
    {
        let ops = const {
            &Ops {
                kind: TypeId::of::<(T, F)>(),
                equal: |left, a, right, b| {
                    // safety: insertion pairs this table with the stored tuple
                    let (a, _) = unsafe { left.data.get(Offset::<(T, F)>::new(a)) };
                    let (b, _) = unsafe { right.data.get(Offset::<(T, F)>::new(b)) };
                    T::equal(a, left, b, right)
                },
                hash: |list, offset, state| {
                    // safety: insertion pairs this table with the stored tuple
                    let (value, _) = unsafe { list.data.get(Offset::<(T, F)>::new(offset)) };
                    T::hash(value, list, state);
                },
                dispatch: |command, list, context| {
                    // safety: dispatch receives the tuple and backend context paired with this table
                    let (value, prepare) = unsafe { list.data.get(Offset::<(T, F)>::new(command.value)) };
                    let offset = command.value + offset_of!((T, F), 0) as u32;
                    let context = unsafe { context.cast::<C>().as_mut() };
                    prepare(value, offset, command, list, context);
                },
            }
        };
        // safety: the table matches this tuple and the caller guarantees the backend context
        unsafe {
            self.push(bounds, clip, (value, prepare), ops);
        }
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
        unsafe { (operations.hash)(self, command.value, state) };
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
        unsafe { (operations.equal)(self, left.value, other, right.value) }
    }

    pub fn clear(&mut self) {
        self.storage.clear();
        self.commands.clear();
        self.clips.clear();
        self.data.clear();
    }
}

pub struct Command {
    pub bounds: PhysicalRect,
    pub clip: u32,
    value: u32,
    ops: &'static Ops,
}

impl Command {
    /// # Safety
    /// list must contain this command and context must match its backend
    #[inline]
    pub unsafe fn dispatch(&self, list: &DisplayList, context: NonNull<()>) {
        // safety: the caller provides this command's list and backend context
        unsafe { (self.ops.dispatch)(self, list, context) };
    }

    pub fn offset(&self) -> u32 {
        self.value
    }
}

/// shared comparison operations and backend dispatch for one input type
pub struct Ops {
    pub kind: TypeId,
    pub equal: unsafe fn(&DisplayList, u32, &DisplayList, u32) -> bool,
    pub hash: unsafe fn(&DisplayList, u32, &mut FxHasher),
    pub dispatch: unsafe fn(&Command, &DisplayList, NonNull<()>),
}

#[derive(Clone, Copy)]
pub struct Clip {
    pub parent: u32,
    pub area: LogicalRect,
    pub radius: BorderRadius,
}
