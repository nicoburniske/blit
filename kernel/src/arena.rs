use std::{
    cell::{Cell, UnsafeCell},
    mem::{MaybeUninit, align_of, needs_drop, size_of},
    ops::{Deref, DerefMut},
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
    scratch_used: Cell<usize>,
    scratch_peak: Cell<usize>,
    scratch_live: Cell<usize>,
}

impl DataArena {
    pub fn store<T: 'static>(&mut self, value: T) -> DataId {
        const {
            assert!(
                align_of::<T>() <= align_of::<Word>(),
                "frame data alignment exceeds arena alignment"
            );
        }
        // power of two alignment lets us round up without division
        let mask = align_of::<T>() - 1;
        let offset = self.len.checked_add(mask).expect("too much frame data") & !mask;
        let end = offset.checked_add(size_of::<T>()).expect("too much frame data");
        assert!(offset < u32::MAX as usize, "too much frame data");
        let id = DataId(offset as u32);
        assert!(end <= isize::MAX as usize, "too much frame data");
        self.words.resize_with(end.div_ceil(size_of::<Word>()), || {
            Word(UnsafeCell::new(MaybeUninit::uninit()))
        });
        self.len = end;
        let pointer = self.pointer::<T>(id);
        // record the destructor while rust owns value so a panic in push still drops it
        // the pointer is checked above and the following write cannot panic
        if const { needs_drop::<T>() } {
            self.drops.push(DropEntry {
                offset: id.0,
                drop: drop_value::<T>,
            });
        }
        // safety: the backing words are aligned and sized for T
        unsafe { pointer.write(value) };
        id
    }

    pub fn scratch<T: Copy>(&self, len: usize, value: T) -> Scratch<'_, T> {
        assert!(
            align_of::<T>() <= align_of::<Word>(),
            "scratch alignment exceeds arena alignment"
        );
        let start = self.scratch_used.get().max(self.len);
        let offset = start
            .checked_next_multiple_of(align_of::<T>())
            .expect("too much scratch data");
        let bytes = size_of::<T>().checked_mul(len).expect("too much scratch data");
        let end = offset.checked_add(bytes).expect("too much scratch data");
        assert!(end <= isize::MAX as usize, "too much scratch data");
        let live = self.scratch_live.get().checked_add(1).expect("too many scratch guards");
        let inner = if end <= self.words.len() * size_of::<Word>() {
            let pointer = if len == 0 || size_of::<T>() == 0 {
                std::ptr::NonNull::<T>::dangling().as_ptr()
            } else {
                // safety: this range is disjoint from retained data and all live scratch guards
                unsafe {
                    UnsafeCell::raw_get(self.words.as_ptr().cast::<UnsafeCell<MaybeUninit<[u8; 64]>>>())
                        .cast::<u8>()
                        .add(offset)
                        .cast::<T>()
                }
            };
            // safety: the storage is aligned and sized for this slice
            let values = unsafe { std::slice::from_raw_parts_mut(pointer.cast::<MaybeUninit<T>>(), len) };
            values.fill(MaybeUninit::new(value));
            // safety: every element has been initialized
            ScratchInner::Ref(unsafe { std::slice::from_raw_parts_mut(pointer, len) })
        } else {
            ScratchInner::Owned(vec![value; len])
        };
        self.scratch_used.set(end);
        self.scratch_peak.set(self.scratch_peak.get().max(end));
        self.scratch_live.set(live);
        Scratch {
            inner,
            arena: self,
            start,
            end,
        }
    }

    pub fn prepare_scratch(&mut self) {
        let needed = self.scratch_peak.get().div_ceil(size_of::<Word>());
        self.words.reserve(needed.saturating_sub(self.words.len()));
        self.words
            .resize_with(self.words.capacity(), || Word(UnsafeCell::new(MaybeUninit::uninit())));
        self.scratch_used.set(self.len);
        self.scratch_live.set(0);
    }

    pub fn load<T: 'static>(&self, id: DataId) -> &T {
        // safety: store wrote T at this checked address
        unsafe { &*self.pointer::<T>(id) }
    }

    pub fn load_mut<T: 'static>(&mut self, id: DataId) -> &mut T {
        // safety: the mutable arena borrow makes this checked address exclusive
        unsafe { &mut *self.pointer::<T>(id) }
    }

    pub fn clear(&mut self) {
        let data = self.words.as_mut_ptr().cast::<u8>();
        while let Some(entry) = self.drops.pop() {
            // safety: entries point to initialized values in the arena
            (entry.drop)(unsafe { data.add(entry.offset as usize) });
        }
        self.len = 0;
    }
}

impl DataArena {
    fn pointer<T>(&self, id: DataId) -> *mut T {
        let offset = id.offset().expect("frame data is missing");
        assert!(offset.checked_add(size_of::<T>()).is_some_and(|end| end <= self.len));
        if size_of::<T>() == 0 {
            return std::ptr::NonNull::<T>::dangling().as_ptr();
        }
        // safety: the offset is within the arena and shared writes use disjoint reserved ranges
        unsafe {
            UnsafeCell::raw_get(self.words.as_ptr().cast::<UnsafeCell<MaybeUninit<[u8; 64]>>>())
                .cast::<u8>()
                .add(offset)
                .cast::<T>()
        }
    }
}

impl Drop for DataArena {
    fn drop(&mut self) {
        self.clear();
    }
}

/// temporary slice reclaimed when its guard drops
pub struct Scratch<'a, T> {
    inner: ScratchInner<'a, T>,
    arena: &'a DataArena,
    start: usize,
    end: usize,
}

impl<T> Deref for Scratch<'_, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        match &self.inner {
            ScratchInner::Ref(values) => values,
            ScratchInner::Owned(values) => values,
        }
    }
}

impl<T> DerefMut for Scratch<'_, T> {
    fn deref_mut(&mut self) -> &mut [T] {
        match &mut self.inner {
            ScratchInner::Ref(values) => values,
            ScratchInner::Owned(values) => values,
        }
    }
}

impl<T> Drop for Scratch<'_, T> {
    fn drop(&mut self) {
        let live = self.arena.scratch_live.get() - 1;
        self.arena.scratch_live.set(live);
        // gaps from guards dropped out of order remain until the last guard drops
        if live == 0 {
            self.arena.scratch_used.set(self.arena.len);
        } else if self.arena.scratch_used.get() == self.end {
            self.arena.scratch_used.set(self.start);
        }
    }
}

enum ScratchInner<'a, T> {
    Ref(&'a mut [T]),
    Owned(Vec<T>),
}

struct DropEntry {
    offset: u32,
    drop: fn(*mut u8),
}

fn drop_value<T>(value: *mut u8) {
    // safety: the drop entry was created for T at this address
    unsafe { value.cast::<T>().drop_in_place() };
}

#[repr(C, align(64))]
struct Word(UnsafeCell<MaybeUninit<[u8; 64]>>);

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;

    #[test]
    fn drops_owned_values_and_skips_trivial_values() {
        #[repr(align(64))]
        struct Aligned(u8);

        struct Dropped(Rc<Cell<bool>>);

        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }

        let dropped = Rc::new(Cell::new(false));
        let mut arena = DataArena::default();
        arena.store(1_u32);
        let aligned = arena.store(Aligned(7));
        assert_eq!(arena.load::<Aligned>(aligned).0, 7);
        assert!(arena.drops.is_empty());
        arena.store(Dropped(dropped.clone()));
        assert_eq!(arena.drops.len(), 1);

        arena.clear();
        assert!(dropped.get());
        assert!(arena.drops.is_empty());
    }
}
