use std::{
    cell::UnsafeCell,
    mem::{MaybeUninit, align_of, needs_drop, size_of},
};

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct DataId(u32);

impl DataId {
    pub const NONE: Self = Self(u32::MAX);

    pub fn offset(self) -> Option<usize> {
        (self.0 != u32::MAX).then_some(self.0 as usize)
    }
}

#[derive(Default)]
pub struct DataArena {
    words: Vec<Word>,
    drops: Vec<DropEntry>,
    len: usize,
}

impl DataArena {
    pub fn store_slice<T: Copy + 'static>(&mut self, len: usize, value: T) -> DataId {
        assert!(align_of::<T>() <= align_of::<Word>());
        let offset = self.len.checked_next_multiple_of(align_of::<T>()).unwrap();
        let end = offset.checked_add(size_of::<T>().checked_mul(len).unwrap()).unwrap();
        self.words.resize_with(end.div_ceil(size_of::<Word>()), || {
            Word(UnsafeCell::new(MaybeUninit::uninit()))
        });
        let id = DataId(u32::try_from(offset).unwrap());
        // safety: the backing words are aligned and sized for the slice
        unsafe {
            let pointer = self.words.as_mut_ptr().cast::<u8>().add(offset).cast::<T>();
            for index in 0..len {
                pointer.add(index).write(value);
            }
        }
        self.len = end;
        id
    }

    /// # Safety
    /// id must point to a live value of type T with no overlapping mutable borrow
    pub unsafe fn load_unchecked<T: 'static>(&self, id: DataId) -> &T {
        debug_assert!(id.offset().is_some());
        debug_assert_eq!(id.0 as usize % align_of::<T>(), 0);
        debug_assert!(id.0 as usize + size_of::<T>() <= self.len);
        unsafe { &*self.words.as_ptr().cast::<u8>().add(id.0 as usize).cast::<T>() }
    }

    /// # Safety
    /// id and len must describe a live initialized slice of T
    pub unsafe fn slice_ptr<T: Copy + 'static>(&self, id: DataId, len: usize) -> *mut [T] {
        debug_assert_eq!(id.0 as usize % align_of::<T>(), 0);
        debug_assert!(id.0 as usize + size_of::<T>() * len <= self.len);
        unsafe {
            let data = UnsafeCell::raw_get(self.words.as_ptr().cast::<UnsafeCell<MaybeUninit<[u8; 64]>>>());
            std::ptr::slice_from_raw_parts_mut(data.cast::<u8>().add(id.0 as usize).cast::<T>(), len)
        }
    }

    pub fn store<T: 'static>(&mut self, value: T) -> DataId {
        const {
            assert!(
                align_of::<T>() <= align_of::<Word>(),
                "frame data alignment exceeds arena alignment"
            );
        }
        let offset = self
            .len
            .checked_next_multiple_of(align_of::<T>())
            .expect("too much frame data");
        let end = offset.checked_add(size_of::<T>().max(1)).expect("too much frame data");
        let id = DataId(u32::try_from(offset).expect("too much frame data"));
        let needs_drop = const { needs_drop::<T>() };
        self.words.resize_with(end.div_ceil(size_of::<Word>()), || {
            Word(UnsafeCell::new(MaybeUninit::uninit()))
        });
        if needs_drop {
            self.drops.reserve(1);
        }
        // safety: the backing words are aligned and sized for T
        unsafe {
            self.words
                .as_mut_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<T>()
                .write(value)
        };
        if needs_drop {
            self.drops.push(DropEntry {
                offset: id.0,
                drop: drop_value::<T>,
            });
        }
        self.len = end;
        id
    }

    pub fn load<T: 'static>(&self, id: DataId) -> &T {
        let offset = self.offset::<T>(id);
        // safety: store wrote T at this checked address
        unsafe { &*self.words.as_ptr().cast::<u8>().add(offset).cast::<T>() }
    }

    pub fn load_mut<T: 'static>(&mut self, id: DataId) -> &mut T {
        let offset = self.offset::<T>(id);
        // safety: the mutable arena borrow makes this checked address exclusive
        unsafe { &mut *self.words.as_mut_ptr().cast::<u8>().add(offset).cast::<T>() }
    }

    fn offset<T: 'static>(&self, id: DataId) -> usize {
        let offset = id.offset().expect("frame data is missing");
        assert!(align_of::<T>() <= align_of::<Word>());
        assert_eq!(offset % align_of::<T>(), 0);
        assert!(offset.checked_add(size_of::<T>()).is_some_and(|end| end <= self.len));
        offset
    }

    pub fn bytes(&self) -> usize {
        self.len
    }

    /// position must be a checkpoint from bytes between allocations
    #[inline]
    pub fn rewind(&mut self, position: usize) {
        debug_assert!(position <= self.len);
        let data = self.words.as_mut_ptr().cast::<u8>();
        while self.drops.last().is_some_and(|entry| entry.offset as usize >= position) {
            let entry = self.drops.pop().unwrap();
            // safety: entries point to initialized values after the checkpoint
            unsafe { (entry.drop)(data.add(entry.offset as usize)) };
        }
        self.len = position;
    }

    pub fn clear(&mut self) {
        self.rewind(0);
    }
}

impl Drop for DataArena {
    fn drop(&mut self) {
        self.clear();
    }
}

struct DropEntry {
    offset: u32,
    drop: unsafe fn(*mut u8),
}

unsafe fn drop_value<T>(value: *mut u8) {
    // safety: the drop entry was created for T at this address
    unsafe { value.cast::<T>().drop_in_place() };
}

#[repr(C, align(64))]
struct Word(UnsafeCell<MaybeUninit<[u8; 64]>>);

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    #[test]
    fn reclaims_owned_values_and_preserves_values_before_the_checkpoint() {
        #[repr(align(64))]
        struct Aligned(u8);

        let value = Rc::new(());
        let mut arena = DataArena::default();
        let aligned = arena.store(Aligned(7));
        assert_eq!(arena.load::<Aligned>(aligned).0, 7);
        assert!(arena.drops.is_empty());
        let earlier = arena.store(value.clone());
        let checkpoint = arena.bytes();
        arena.store(value.clone());
        assert_eq!(Rc::strong_count(&value), 3);

        arena.rewind(checkpoint);
        assert_eq!(Rc::strong_count(&value), 2);
        assert_eq!(arena.bytes(), checkpoint);
        assert!(Rc::ptr_eq(arena.load::<Rc<()>>(earlier), &value));

        arena.clear();
        assert_eq!(Rc::strong_count(&value), 1);
        assert_eq!(arena.bytes(), 0);
    }
}
