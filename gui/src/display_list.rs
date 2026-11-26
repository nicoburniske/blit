use alloc::vec::Vec;
use core::{
    any::TypeId,
    hash::{BuildHasher, Hash, Hasher},
    mem::offset_of,
};

use blit::{FrameStore, FrameStoreMut, LogicalRect, PhysicalRect};

use crate::style::BorderRadius;

pub type DisplayHasher = <hashbrown::DefaultHashBuilder as BuildHasher>::Hasher;

/// owned drawing inputs and their replay callbacks
#[derive(Default)]
pub struct DisplayList {
    commands: Vec<Command>,
    clips: Vec<Clip>,
    store: FrameStore,
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

    pub fn store(&self) -> &FrameStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> FrameStoreMut<'_> {
        self.store.as_mut()
    }

    /// records a value and a callback with the backend's replay signature
    ///
    /// use distinct input types for commands with different rendering behavior
    #[inline]
    pub fn push<T: Compare, F: Copy + 'static>(
        &mut self,
        area: LogicalRect,
        bounds: PhysicalRect,
        clip: u32,
        value: T,
        replay: F,
    ) {
        assert!(clip as usize <= self.clips.len(), "invalid paint clip");
        let ops = const {
            &Ops {
                kind: TypeId::of::<T>(),
                replay: TypeId::of::<F>(),
                input_offset: offset_of!((T, F), 0) as u32,
                replay_offset: offset_of!((T, F), 1) as u32,
                equal: |left, a, right, b| {
                    // safety: equivalent checks both input types
                    let a = unsafe { left.store.get::<T>(a) };
                    let b = unsafe { right.store.get::<T>(b) };
                    T::equal(a, left, b, right)
                },
                hash: |list, offset, state| {
                    // safety: push pairs the input type and hash operation
                    let value = unsafe { list.store.get::<T>(offset) };
                    T::hash(value, list, state);
                },
            }
        };
        self.commands.push(Command {
            area,
            bounds,
            clip,
            offset: self.store.push((value, replay)),
            ops,
        });
    }

    /// returns the recorded value and its offset in the list's store
    #[inline]
    pub fn input<T: 'static>(&self, index: usize) -> (&T, u32) {
        let command = &self.commands[index];
        assert_eq!(command.ops.kind, TypeId::of::<T>(), "wrong command input type");
        let offset = command.offset + command.ops.input_offset;
        // safety: push stores the value of the checked type at this offset
        (unsafe { self.store.get::<T>(offset) }, offset)
    }

    /// returns the callback without erasing any of its arguments
    #[inline]
    pub fn replay<F: Copy + 'static>(&self, index: usize) -> F {
        let command = &self.commands[index];
        assert_eq!(command.ops.replay, TypeId::of::<F>(), "wrong command replay signature");
        // safety: push stores the callback of the checked type at this offset
        *unsafe { self.store.get::<F>(command.offset + command.ops.replay_offset) }
    }

    pub fn push_clip(&mut self, clip: Clip) -> u32 {
        assert!(clip.parent as usize <= self.clips.len(), "invalid parent clip");
        let index = u32::try_from(self.clips.len()).expect("too many paint clips");
        let index = index.checked_add(1).expect("too many paint clips");
        self.clips.push(clip);
        index
    }

    pub fn hash(&self, index: usize, state: &mut DisplayHasher) {
        let command = &self.commands[index];
        let operations = command.ops;
        operations.kind.hash(state);
        let area = command.area;
        [area.x, area.y, area.width, area.height]
            .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
            .hash(state);
        // safety: insertion pairs the operations with their recorded data
        unsafe { (operations.hash)(self, command.offset + operations.input_offset, state) };
    }

    pub fn equivalent(&self, index: usize, other: &Self, other_index: usize) -> bool {
        let left = &self.commands[index];
        let right = &other.commands[other_index];
        let operations = left.ops;
        if left.area != right.area || left.bounds != right.bounds || operations.kind != right.ops.kind {
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
        // safety: the matching input types identify the representation at each input offset
        unsafe {
            (operations.equal)(
                self,
                left.offset + operations.input_offset,
                other,
                right.offset + right.ops.input_offset,
            )
        }
    }

    pub fn clear(&mut self) {
        self.commands.clear();
        self.clips.clear();
        self.store.clear();
    }
}

pub struct Command {
    /// logical layout area given to paint
    pub area: LogicalRect,
    /// physical pixels the command may touch
    pub bounds: PhysicalRect,
    pub clip: u32,
    offset: u32,
    ops: &'static Ops,
}

struct Ops {
    kind: TypeId,
    replay: TypeId,
    input_offset: u32,
    replay_offset: u32,
    equal: unsafe fn(&DisplayList, u32, &DisplayList, u32) -> bool,
    hash: unsafe fn(&DisplayList, u32, &mut DisplayHasher),
}

#[derive(Clone, Copy)]
pub struct Clip {
    pub parent: u32,
    pub area: LogicalRect,
    pub radius: BorderRadius,
}
