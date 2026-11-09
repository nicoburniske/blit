use std::{any::TypeId, hash::Hash, marker::PhantomData, ptr::NonNull};

use blit::{LogicalRect, PhysicalRect, internal::hash::FxHasher};

use crate::{
    arena::{Arena, Writer},
    style::BorderRadius,
};

/// owned drawing inputs and their typed operations
pub struct DisplayList<B: 'static> {
    commands: Vec<Command<B>>,
    clips: Vec<Clip>,
    data: Arena,
}

pub struct Command<B: 'static> {
    pub bounds: PhysicalRect,
    pub clip: u32,
    value: u32,
    ops: NonNull<ErasedOperations<B>>,
}

/// typed operations for one recorded input type
#[repr(C)]
pub struct Operations<T, B, E, H> {
    erased: ErasedOperations<B>,
    equal: E,
    hash: H,
    marker: PhantomData<fn(T) -> T>,
}

struct ErasedOperations<B> {
    kind: TypeId,
    equal: unsafe fn(NonNull<Self>, &Arena, u32, &Arena, u32) -> bool,
    hash: unsafe fn(NonNull<Self>, &Arena, u32, &mut FxHasher),
    backend: B,
}

#[derive(Clone, Copy)]
pub struct Clip {
    pub parent: u32,
    pub area: LogicalRect,
    pub radius: BorderRadius,
}

impl<B: 'static> DisplayList<B> {
    pub fn commands(&self) -> &[Command<B>] {
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

    /// records owned data with operations for the same type
    ///
    /// ```compile_fail,E0308
    /// use std::hash::Hash;
    /// use blit::PhysicalRect;
    /// use blit_gui::display_list::{DisplayList, Operations};
    ///
    /// let operations = const {
    ///     &Operations::new(
    ///         |a: &u32, _, b, _| a == b,
    ///         |value, _, state| value.hash(state),
    ///         (),
    ///     )
    /// };
    /// let mut list = DisplayList::default();
    /// list.push(PhysicalRect::default(), 0, "wrong type", operations);
    /// ```
    pub fn push<T: 'static, E: 'static, H: 'static>(
        &mut self,
        bounds: PhysicalRect,
        clip: u32,
        value: T,
        operations: &'static Operations<T, B, E, H>,
    ) {
        assert!(clip as usize <= self.clips.len(), "invalid paint clip");
        self.commands.push(Command {
            bounds,
            clip,
            value: self.data.push_erased(value),
            ops: NonNull::from(operations).cast(),
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
        let operations = command.operations();
        operations.kind.hash(state);
        // safety: insertion pairs the operations with their recorded data
        unsafe { (operations.hash)(command.ops, &self.data, command.value, state) };
    }

    pub fn equivalent(&self, index: usize, other: &Self, other_index: usize) -> bool {
        let left = &self.commands[index];
        let right = &other.commands[other_index];
        let operations = left.operations();
        if left.bounds != right.bounds || operations.kind != right.operations().kind {
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
        unsafe { (operations.equal)(left.ops, &self.data, left.value, &other.data, right.value) }
    }

    pub fn clear(&mut self) {
        self.commands.clear();
        self.clips.clear();
        self.data.clear();
    }
}

impl<B: 'static> Command<B> {
    pub fn backend(&self) -> &B {
        &self.operations().backend
    }

    pub fn offset(&self) -> u32 {
        self.value
    }

    fn operations(&self) -> &ErasedOperations<B> {
        // safety: push stores a static typed table with its erased entry at the start
        unsafe { self.ops.as_ref() }
    }
}

impl<T: 'static, B, E, H> Operations<T, B, E, H>
where
    E: Fn(&T, &Arena, &T, &Arena) -> bool + 'static,
    H: Fn(&T, &Arena, &mut FxHasher) + 'static,
{
    /// tables of the same type must compare and hash identically
    pub const fn new(equal: E, hash: H, backend: B) -> Self {
        Self {
            erased: ErasedOperations {
                kind: TypeId::of::<(T, E, H)>(),
                equal: |operations, left, a, right, b| {
                    // safety: the erased entry is the first field of this typed table
                    let operations = unsafe { operations.cast::<Self>().as_ref() };
                    // safety: recording pairs the table with T in each list
                    let a = unsafe { left.value_unchecked::<T>(a) };
                    let b = unsafe { right.value_unchecked::<T>(b) };
                    (operations.equal)(a, left, b, right)
                },
                hash: |operations, data, offset, state| {
                    // safety: the erased entry is the first field of this typed table
                    let operations = unsafe { operations.cast::<Self>().as_ref() };
                    // safety: recording pairs the table with T
                    let value = unsafe { data.value_unchecked::<T>(offset) };
                    (operations.hash)(value, data, state);
                },
                backend,
            },
            equal,
            hash,
            marker: PhantomData,
        }
    }
}

impl<B: 'static> Default for DisplayList<B> {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
            clips: Vec::new(),
            data: Arena::default(),
        }
    }
}
