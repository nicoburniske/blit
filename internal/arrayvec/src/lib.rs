use std::{
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    slice,
};

pub struct ArrayVec<T, const N: usize> {
    values: [MaybeUninit<T>; N],
    len: usize,
}

impl<T, const N: usize> ArrayVec<T, N> {
    #[inline]
    pub const fn new() -> Self {
        Self {
            values: [const { MaybeUninit::uninit() }; N],
            len: 0,
        }
    }

    #[inline]
    pub fn push(&mut self, value: T) {
        assert!(!self.is_full(), "array capacity exceeded");
        self.values[self.len].write(value);
        self.len += 1;
    }

    #[inline]
    pub fn clear(&mut self) {
        self.truncate(0);
    }

    #[inline]
    pub fn truncate(&mut self, len: usize) {
        if !std::mem::needs_drop::<T>() {
            self.len = self.len.min(len);
            return;
        }
        while self.len > len {
            self.len -= 1;
            unsafe { self.values[self.len].assume_init_drop() };
        }
    }

    #[inline]
    pub fn extend_from_slice(&mut self, values: &[T])
    where
        T: Copy,
    {
        assert!(values.len() <= N - self.len, "array capacity exceeded");
        for &value in values {
            self.push(value);
        }
    }

    #[inline]
    pub const fn is_full(&self) -> bool {
        self.len == N
    }

    #[inline]
    pub fn spare_capacity_mut(&mut self) -> &mut [MaybeUninit<T>] {
        &mut self.values[self.len..]
    }

    /// # Safety
    ///
    /// elements between the old and new lengths must be initialized
    #[inline]
    pub unsafe fn set_len(&mut self, len: usize) {
        assert!(len >= self.len && len <= N);
        self.len = len;
    }
}

impl<T, const N: usize> Default for ArrayVec<T, N> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> Deref for ArrayVec<T, N> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.values.as_ptr().cast(), self.len) }
    }
}

impl<T, const N: usize> DerefMut for ArrayVec<T, N> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.values.as_mut_ptr().cast(), self.len) }
    }
}

impl<T, const N: usize> Drop for ArrayVec<T, N> {
    #[inline]
    fn drop(&mut self) {
        self.clear();
    }
}
