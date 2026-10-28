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
    /// the offset must identify a live initialized T in this storage
    #[inline]
    pub unsafe fn get_mut<T>(&mut self, offset: Offset<T>) -> &mut T {
        // safety: the caller guarantees the type and validity and the mutable borrow is exclusive
        unsafe { &mut *self.as_ptr().add(offset.byte as usize).cast::<T>() }
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
    pub const fn new(byte: u32) -> Self {
        Self {
            byte,
            marker: PhantomData,
        }
    }

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
