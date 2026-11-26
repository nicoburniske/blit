use alloc::vec::Vec;
use core::{
    cell::Cell,
    fmt::{self, Write as _},
    mem::{MaybeUninit, align_of, size_of},
    ops::{Deref, DerefMut},
};

use crate::internal::bump::Bump;

/// reusable storage for temporary slices and formatted text
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
                core::ptr::NonNull::<T>::dangling().as_ptr()
            } else {
                // safety: this range is disjoint from all live scratch guards
                unsafe { self.bump.as_ptr().add(offset).cast::<T>() }
            };
            // safety: the storage is aligned and sized for this uninitialized slice
            ScratchInner::Ref(unsafe { core::slice::from_raw_parts_mut(pointer.cast::<MaybeUninit<T>>(), capacity) })
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

    /// formats arguments into temporary scratch storage
    pub fn fmt(&self, arguments: fmt::Arguments<'_>) -> ScratchStr<'_> {
        struct Writer<'a, 'b>(&'b mut Scratch<'a, u8>);

        impl fmt::Write for Writer<'_, '_> {
            fn write_str(&mut self, text: &str) -> fmt::Result {
                let bytes = text.as_bytes();
                let new_len = self.0.len + bytes.len();
                if new_len > self.0.capacity() {
                    match &mut self.0.inner {
                        ScratchInner::Owned(values) => values.resize(new_len, MaybeUninit::uninit()),
                        ScratchInner::Ref(slice) => {
                            let mut owned = Vec::with_capacity(new_len);
                            owned.extend_from_slice(&slice[..self.0.len]);
                            owned.resize(new_len, MaybeUninit::uninit());
                            self.0.inner = ScratchInner::Owned(owned);
                            if self.0.buffer.used.get() == self.0.end {
                                self.0.buffer.used.set(self.0.start);
                            }
                        }
                    }
                }
                self.0.inner.as_mut_slice()[self.0.len..new_len].write_copy_of_slice(bytes);
                self.0.len = new_len;
                Ok(())
            }
        }

        let start = self.used.get().max(self.bump.used());
        let mut scratch = self.alloc::<u8>(self.bump.capacity().saturating_sub(start));
        let _ = Writer(&mut scratch).write_fmt(arguments);
        scratch.end = scratch.start + scratch.len;
        if matches!(scratch.inner, ScratchInner::Ref(_)) {
            self.used.set(scratch.end);
        }
        self.peak.set(self.peak.get().max(scratch.end));
        ScratchStr(scratch)
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
        self.inner.as_mut_slice()[self.len].write(value);
        self.len += 1;
    }

    pub fn resize(&mut self, len: usize, value: T) {
        if len > self.len {
            self.inner.as_mut_slice()[self.len..len].fill(MaybeUninit::new(value));
        }
        self.len = len;
    }

    pub fn pop(&mut self) -> Option<T> {
        let value = *self.last()?;
        self.len -= 1;
        Some(value)
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }
}

impl<T: Copy> Deref for Scratch<'_, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        // safety: len only includes initialized elements
        unsafe { self.inner.as_slice()[..self.len].assume_init_ref() }
    }
}

impl<T: Copy> Extend<T> for Scratch<'_, T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for value in iter {
            self.push(value);
        }
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

/// temporary formatted text backed by a [`ScratchBuffer`]
pub struct ScratchStr<'a>(Scratch<'a, u8>);

impl Deref for ScratchStr<'_> {
    type Target = str;

    fn deref(&self) -> &str {
        // safety: fmt only writes valid utf-8 slices into the backing scratch buffer
        unsafe { core::str::from_utf8_unchecked(&self.0) }
    }
}

impl AsRef<str> for ScratchStr<'_> {
    fn as_ref(&self) -> &str {
        self
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
