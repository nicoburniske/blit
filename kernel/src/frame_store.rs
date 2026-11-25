use core::{
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicU32, Ordering},
};

use crate::internal::bump::{Bump, Offset, Slice};

/// provides access to drawing inputs owned by a context
pub trait HasFrameStore {
    fn store(&self) -> &FrameStore;
    fn store_mut(&mut self) -> FrameStoreMut<'_>;
}

/// drawing inputs owned by a rendering context until it clears them
#[derive(Default)]
pub struct FrameStore {
    data: Bump,
    epoch: u32,
}

/// append-only mutable access to a [`FrameStore`]
pub struct FrameStoreMut<'a>(&'a mut FrameStore);

impl<'a> FrameStoreMut<'a> {
    /// reserves uninitialized drawing inputs until this storage is cleared
    pub fn slice<T: Copy + 'static>(self, capacity: usize) -> FrameSliceBuilder<'a, T> {
        u32::try_from(capacity).expect("too much frame data");
        static NEXT_EPOCH: AtomicU32 = AtomicU32::new(1);
        if self.0.epoch == 0 {
            let mut epoch = NEXT_EPOCH.fetch_add(1, Ordering::Relaxed);
            if epoch == 0 {
                epoch = NEXT_EPOCH.fetch_add(1, Ordering::Relaxed);
            }
            self.0.epoch = epoch;
        }
        let offset = self.0.data.allocate::<T>(capacity);
        // safety: allocate reserves aligned storage and this borrow prevents growth or clearing
        let values = unsafe {
            core::slice::from_raw_parts_mut(self.0.data.as_ptr().add(offset.byte() as usize).cast(), capacity)
        };
        FrameSliceBuilder {
            values,
            slice: Slice { offset, len: 0 },
            epoch: self.0.epoch,
        }
    }

    /// copies values into storage until this storage is cleared
    pub fn slice_from<T: Copy + 'static>(&mut self, values: &[T]) -> FrameSlice<T> {
        let mut slice = FrameStoreMut(self.0).slice(values.len());
        slice.values.write_copy_of_slice(values);
        slice.slice.len = values.len() as u32;
        slice.finish()
    }
}

impl Deref for FrameStoreMut<'_> {
    type Target = FrameStore;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

/// a typed slice resolved through the FrameStore that allocated it
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameSlice<T> {
    slice: Slice<T>,
    epoch: u32,
}

/// reserved frame storage exposing only its initialized prefix
///
/// ```
/// let mut frame = blit::FrameStore::default();
/// let mut values = frame.as_mut().slice(4);
/// values.extend([3, 1, 2]);
/// values.sort_unstable();
/// let values = values.finish();
/// assert_eq!(frame.view(values), &[1, 2, 3]);
/// ```
///
/// storage cannot grow or clear while a builder is open
///
/// ```compile_fail
/// let mut frame = blit::FrameStore::default();
/// let mut values = frame.as_mut().slice(4);
/// frame.clear();
/// values.push(1u32);
/// ```
pub struct FrameSliceBuilder<'a, T: Copy> {
    values: &'a mut [MaybeUninit<T>],
    slice: Slice<T>,
    epoch: u32,
}

impl FrameStore {
    pub fn view<T: Copy + 'static>(&self, slice: FrameSlice<T>) -> &[T] {
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

    /// returns append-only mutable access without exposing clearing or replacement
    pub fn as_mut(&mut self) -> FrameStoreMut<'_> {
        FrameStoreMut(self)
    }

    /// stores a value until this storage is cleared and returns its byte offset
    #[inline]
    pub fn push<T: 'static>(&mut self, value: T) -> u32 {
        self.data.push(value).byte()
    }

    /// # Safety
    /// `offset` must identify a live initialized `T` pushed into this store
    #[inline]
    pub unsafe fn get<T>(&self, offset: u32) -> &T {
        // safety: the caller guarantees the type and validity of this offset
        unsafe { self.data.get(Offset::new(offset)) }
    }

    /// bytes currently occupied in this store
    #[inline]
    pub fn used(&self) -> usize {
        self.data.used()
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

    pub fn clear(&mut self) {
        self.slice.len = 0;
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
        let values = values.into_iter();
        let start = self.slice.len as usize;
        let slots = &mut self.values[start..];
        let (lower, upper) = values.size_hint();
        assert!(lower <= slots.len(), "frame slice capacity exceeded");
        // an exact hint trims the slots so the indexed writes need no capacity check
        let slots = if upper == Some(lower) {
            &mut slots[..lower]
        } else {
            slots
        };
        let mut written = 0;
        values.for_each(|value| {
            slots[written].write(value);
            written += 1;
        });
        // committed last so a panicking iterator never counts unwritten slots
        self.slice.len = (start + written) as u32;
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
