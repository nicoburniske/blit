//! aligned storage addressed by byte offsets

use std::{
    cell::UnsafeCell,
    marker::PhantomData,
    mem::{MaybeUninit, align_of, size_of},
};

/// byte storage whose values and destructors are managed by the caller
#[derive(Default)]
pub struct Bump {
    blocks: Vec<Block>,
    len: u32,
}

impl Bump {
    pub const ALIGNMENT: usize = align_of::<Block>();

    #[inline]
    pub fn push<T>(&mut self, value: T) -> Offset<T> {
        let offset = self.allocate::<T>(1);
        // safety: allocate reserves aligned storage for this value
        unsafe { self.as_ptr().add(offset.byte as usize).cast::<T>().write(value) };
        offset
    }

    #[inline]
    pub fn extend<T: Copy>(&mut self, mut values: impl ExactSizeIterator<Item = T>) -> Slice<T> {
        let len = u32::try_from(values.len()).expect("too much bump data");
        let offset = self.allocate::<T>(len as usize);
        // safety: allocate reserves aligned storage for this slice
        let slots = unsafe {
            std::slice::from_raw_parts_mut(
                self.as_ptr().add(offset.byte as usize).cast::<MaybeUninit<T>>(),
                len as usize,
            )
        };
        for slot in slots {
            slot.write(values.next().expect("invalid iter len"));
        }
        assert!(values.next().is_none(), "invalid iter len");
        Slice { offset, len }
    }

    /// # Safety
    /// the offset must identify a live initialized T in this storage
    #[inline]
    pub unsafe fn get<T>(&self, offset: Offset<T>) -> &T {
        // safety: the caller guarantees the type and validity of this allocation
        unsafe { &*self.as_ptr().add(offset.byte as usize).cast::<T>() }
    }

    /// # Safety
    /// the slice must identify live initialized values in this storage
    #[inline]
    pub unsafe fn slice<T>(&self, slice: Slice<T>) -> &[T] {
        if slice.len == 0 {
            return &[];
        }
        // safety: the caller guarantees the type and validity of this allocation
        unsafe {
            std::slice::from_raw_parts(
                self.as_ptr().add(slice.offset.byte as usize).cast::<T>(),
                slice.len as usize,
            )
        }
    }

    /// reserves total storage without advancing the allocation cursor
    #[inline]
    pub fn reserve(&mut self, bytes: usize) {
        let blocks = bytes.div_ceil(size_of::<Block>());
        if blocks > self.blocks.len() {
            self.blocks.reserve(blocks - self.blocks.len());
            self.blocks
                .resize_with(self.blocks.capacity(), || Block(UnsafeCell::new(MaybeUninit::uninit())));
        }
    }

    #[inline]
    pub fn as_ptr(&self) -> *mut u8 {
        UnsafeCell::raw_get(self.blocks.as_ptr().cast::<UnsafeCell<MaybeUninit<[u8; 64]>>>()).cast::<u8>()
    }

    #[inline]
    pub fn used(&self) -> usize {
        self.len as usize
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.blocks.len() * size_of::<Block>()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    #[inline]
    fn allocate<T>(&mut self, len: usize) -> Offset<T> {
        const {
            assert!(
                align_of::<T>() <= Self::ALIGNMENT,
                "bump data alignment exceeds storage alignment"
            );
        }
        let start = (self.len as usize)
            .checked_add(align_of::<T>() - 1)
            .expect("too much bump data")
            & !(align_of::<T>() - 1);
        let end = len
            .checked_mul(size_of::<T>())
            .and_then(|bytes| start.checked_add(bytes))
            .and_then(|end| u32::try_from(end).ok())
            .expect("too much bump data");
        self.reserve(end as usize);
        self.len = end;
        Offset {
            byte: start as u32,
            marker: PhantomData,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct Offset<T> {
    byte: u32,
    marker: PhantomData<fn() -> T>,
}

impl<T> Offset<T> {
    pub const fn byte(self) -> u32 {
        self.byte
    }
}

impl<T> Copy for Offset<T> {}

impl<T> Clone for Offset<T> {
    fn clone(&self) -> Self {
        *self
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Slice<T> {
    pub offset: Offset<T>,
    pub len: u32,
}

impl<T> Slice<T> {
    pub const EMPTY: Self = Self {
        offset: Offset {
            byte: 0,
            marker: PhantomData,
        },
        len: 0,
    };
}

impl<T> Copy for Slice<T> {}

impl<T> Clone for Slice<T> {
    fn clone(&self) -> Self {
        *self
    }
}

#[repr(C, align(64))]
struct Block(UnsafeCell<MaybeUninit<[u8; 64]>>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_alignment_and_values_across_growth() {
        #[repr(C, align(64))]
        struct Aligned([u64; 8]);

        let mut bump = Bump::default();
        let first = bump.push(17_u8);
        let second = bump.push(23_u8);
        let numbers = bump.extend([31_u64, 47].into_iter());
        let aligned = bump.push(Aligned([59; 8]));
        assert_eq!(second.byte(), 1);
        assert_eq!(numbers.offset.byte(), 8);
        assert_eq!(aligned.byte(), 64);
        assert_eq!(size_of::<Offset<Aligned>>(), 4);
        assert_eq!(size_of::<Slice<Aligned>>(), 8);
        bump.reserve(4096);
        assert_eq!(bump.as_ptr().addr() % 64, 0);
        // safety: the offsets identify initialized values after growth
        unsafe {
            assert_eq!(*bump.get(first), 17);
            assert_eq!(*bump.get(second), 23);
            assert_eq!(bump.slice(numbers), &[31, 47]);
            assert_eq!(bump.get(aligned).0, [59; 8]);
        }
        let capacity = bump.capacity();
        bump.clear();
        assert_eq!(bump.push(61_u64).byte(), 0);
        assert_eq!(bump.capacity(), capacity);
        // safety: an empty slice does not access storage
        assert!(unsafe { bump.slice(Slice::<u64>::EMPTY) }.is_empty());
    }
}
