use std::{
    cell::Cell,
    mem::{MaybeUninit, align_of, size_of},
    ops::{Deref, DerefMut},
};

use crate::internal::bump::Bump;

/// reusable storage for temporary slices
#[derive(Default)]
pub struct ScratchBuffer {
    pub(crate) bump: Bump,
    used: Cell<usize>,
    peak: Cell<usize>,
    live: Cell<usize>,
}

impl ScratchBuffer {
    /// reserves a fixed capacity without initializing elements
    pub fn alloc<T: Copy>(&self, capacity: usize) -> Scratch<'_, T> {
        const {
            assert!(
                align_of::<T>() <= Bump::ALIGNMENT,
                "scratch alignment exceeds arena alignment"
            );
        }
        let start = self.used.get().max(self.bump.used());
        let offset = start.next_multiple_of(align_of::<T>());
        let end = size_of::<T>()
            .checked_mul(capacity)
            .and_then(|bytes| offset.checked_add(bytes))
            .filter(|&end| end <= isize::MAX as usize)
            .expect("too much scratch data");
        let live = self.live.get().checked_add(1).expect("too many scratch guards");
        let inner = if end <= self.bump.capacity() {
            let pointer = if capacity == 0 || size_of::<T>() == 0 {
                std::ptr::NonNull::<T>::dangling().as_ptr()
            } else {
                // safety: this range is disjoint from all live scratch guards
                unsafe { self.bump.as_ptr().add(offset).cast::<T>() }
            };
            // safety: the storage is aligned and sized for this uninitialized slice
            ScratchInner::Ref(unsafe { std::slice::from_raw_parts_mut(pointer.cast::<MaybeUninit<T>>(), capacity) })
        } else {
            let mut values = Vec::with_capacity(capacity);
            values.resize_with(capacity, MaybeUninit::uninit);
            ScratchInner::Owned(values)
        };
        self.used.set(end);
        self.peak.set(self.peak.get().max(end));
        self.live.set(live);
        Scratch {
            inner,
            len: 0,
            buffer: self,
            start,
            end,
        }
    }

    /// retains enough storage for the previous peak once no guards are borrowed
    pub fn prepare(&mut self) {
        self.bump.reserve(self.peak.get());
        self.used.set(self.bump.used());
        self.live.set(0);
    }
}

/// a fixed capacity temporary buffer exposing only its initialized elements
pub struct Scratch<'a, T: Copy> {
    inner: ScratchInner<'a, T>,
    len: usize,
    buffer: &'a ScratchBuffer,
    start: usize,
    end: usize,
}

impl<T: Copy> Scratch<'_, T> {
    pub fn capacity(&self) -> usize {
        self.inner.as_slice().len()
    }

    pub fn push(&mut self, value: T) {
        self.spare_capacity_mut()[0].write(value);
        self.len += 1;
    }

    pub fn extend(&mut self, mut values: impl ExactSizeIterator<Item = T>) {
        let count = values.len();
        for slot in &mut self.spare_capacity_mut()[..count] {
            slot.write(values.next().expect("invalid iter len"));
        }
        assert!(values.next().is_none(), "invalid iter len");
        self.len += count;
    }

    pub fn resize(&mut self, len: usize, value: T) {
        if len > self.len {
            self.inner.as_mut_slice()[self.len..len].fill(MaybeUninit::new(value));
        }
        self.len = len;
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn spare_capacity_mut(&mut self) -> &mut [MaybeUninit<T>] {
        &mut self.inner.as_mut_slice()[self.len..]
    }

    /// # Safety
    /// every element before len must be initialized
    pub unsafe fn set_len(&mut self, len: usize) {
        assert!(len <= self.capacity(), "scratch capacity exceeded");
        self.len = len;
    }
}

impl<T: Copy> Deref for Scratch<'_, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        // safety: len only includes initialized elements
        unsafe { self.inner.as_slice()[..self.len].assume_init_ref() }
    }
}

impl<T: Copy> DerefMut for Scratch<'_, T> {
    fn deref_mut(&mut self) -> &mut [T] {
        // safety: len only includes initialized elements and this borrow is exclusive
        unsafe { self.inner.as_mut_slice()[..self.len].assume_init_mut() }
    }
}

impl<T: Copy> Drop for Scratch<'_, T> {
    fn drop(&mut self) {
        let live = self.buffer.live.get() - 1;
        self.buffer.live.set(live);
        // gaps from guards dropped out of order remain until the last guard drops
        if live == 0 {
            self.buffer.used.set(self.buffer.bump.used());
        } else if self.buffer.used.get() == self.end {
            self.buffer.used.set(self.start);
        }
    }
}

enum ScratchInner<'a, T> {
    Ref(&'a mut [MaybeUninit<T>]),
    Owned(Vec<MaybeUninit<T>>),
}

impl<T> ScratchInner<'_, T> {
    fn as_slice(&self) -> &[MaybeUninit<T>] {
        match self {
            Self::Ref(values) => values,
            Self::Owned(values) => values,
        }
    }

    fn as_mut_slice(&mut self) -> &mut [MaybeUninit<T>] {
        match self {
            Self::Ref(values) => values,
            Self::Owned(values) => values,
        }
    }
}
