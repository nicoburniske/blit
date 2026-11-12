use std::{
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicU32, Ordering},
};

use crate::internal::bump::{Bump, Slice};

/// drawing inputs owned by a rendering context until it clears them
#[derive(Default)]
pub struct FrameData {
    data: Bump,
    epoch: u32,
}

/// a typed slice resolved through the FrameData that allocated it
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameSlice<T> {
    slice: Slice<T>,
    epoch: u32,
}

/// reserved frame storage exposing only its initialized prefix
///
/// ```
/// let mut frame = blit::FrameData::default();
/// let mut values = frame.slice(4);
/// values.extend([3, 1, 2]);
/// values.sort_unstable();
/// let values = values.finish();
/// assert_eq!(frame.get(values), &[1, 2, 3]);
/// ```
///
/// storage cannot grow or clear while a builder is open
///
/// ```compile_fail
/// let mut frame = blit::FrameData::default();
/// let mut values = frame.slice(4);
/// frame.clear();
/// values.push(1u32);
/// ```
pub struct FrameSliceBuilder<'a, T: Copy> {
    values: &'a mut [MaybeUninit<T>],
    slice: Slice<T>,
    epoch: u32,
}

impl FrameData {
    /// reserves uninitialized drawing inputs until this storage is cleared
    pub fn slice<T: Copy + 'static>(&mut self, capacity: usize) -> FrameSliceBuilder<'_, T> {
        u32::try_from(capacity).expect("too much frame data");
        static NEXT_EPOCH: AtomicU32 = AtomicU32::new(1);
        if self.epoch == 0 {
            self.epoch = NEXT_EPOCH
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |epoch| epoch.checked_add(1))
                .expect("frame data epochs exhausted");
        }
        let offset = self.data.allocate::<T>(capacity);
        // safety: allocate reserves aligned storage and this borrow prevents growth or clearing
        let values =
            unsafe { std::slice::from_raw_parts_mut(self.data.as_ptr().add(offset.byte() as usize).cast(), capacity) };
        FrameSliceBuilder {
            values,
            slice: Slice { offset, len: 0 },
            epoch: self.epoch,
        }
    }

    pub fn get<T: Copy + 'static>(&self, slice: FrameSlice<T>) -> &[T] {
        if slice.slice.len == 0 {
            return &[];
        }
        assert!(
            self.epoch != 0 && self.epoch == slice.epoch,
            "slice belongs to another frame data allocation"
        );
        // safety: the epoch identifies the storage that wrote this typed slice
        unsafe { self.data.slice(slice.slice) }
    }

    /// clears inputs while keeping the allocation for reuse
    pub fn clear(&mut self) {
        self.epoch = 0;
        self.data.clear();
    }
}

impl<T: Copy> FrameSliceBuilder<'_, T> {
    pub fn capacity(&self) -> usize {
        self.values.len()
    }

    pub fn push(&mut self, value: T) {
        self.values[self.slice.len as usize].write(value);
        self.slice.len += 1;
    }

    /// finishes the initialized prefix without copying it
    pub fn finish(self) -> FrameSlice<T> {
        FrameSlice {
            slice: self.slice,
            epoch: self.epoch,
        }
    }
}

impl<T: Copy> Extend<T> for FrameSliceBuilder<'_, T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, values: I) {
        for value in values {
            self.push(value);
        }
    }
}

impl<T: Copy> Deref for FrameSliceBuilder<'_, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        // safety: push initializes each element before extending the prefix
        unsafe { self.values[..self.slice.len as usize].assume_init_ref() }
    }
}

impl<T: Copy> DerefMut for FrameSliceBuilder<'_, T> {
    fn deref_mut(&mut self) -> &mut [T] {
        // safety: the initialized prefix is exclusively borrowed
        unsafe { self.values[..self.slice.len as usize].assume_init_mut() }
    }
}
