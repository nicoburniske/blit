use std::mem::{MaybeUninit, align_of, needs_drop, size_of};

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
    pub fn store<T: 'static>(&mut self, value: T) -> DataId {
        const {
            assert!(align_of::<T>() <= 8, "frame data alignment exceeds 8 bytes");
        }
        let offset = self
            .len
            .checked_next_multiple_of(align_of::<T>())
            .expect("too much frame data");
        let end = offset
            .checked_add(size_of::<T>())
            .expect("too much frame data");
        let id = DataId(u32::try_from(offset).expect("too much frame data"));
        let needs_drop = const { needs_drop::<T>() };
        self.words.resize_with(end.div_ceil(size_of::<Word>()), || {
            Word(MaybeUninit::uninit())
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
        let offset = id.offset().expect("frame data is missing");
        assert!(align_of::<T>() <= 8);
        assert_eq!(offset % align_of::<T>(), 0);
        assert!(
            offset
                .checked_add(size_of::<T>())
                .is_some_and(|end| end <= self.len)
        );
        // safety: store wrote an aligned T at this offset
        unsafe { &*self.words.as_ptr().cast::<u8>().add(offset).cast::<T>() }
    }

    pub fn clear(&mut self) {
        let data = self.words.as_mut_ptr().cast::<u8>();
        while let Some(entry) = self.drops.pop() {
            // safety: entries point to initialized values in the arena
            unsafe { (entry.drop)(data.add(entry.offset as usize)) };
        }
        self.len = 0;
    }

    pub fn heap_bytes(&self) -> usize {
        self.words.capacity() * size_of::<Word>() + self.drops.capacity() * size_of::<DropEntry>()
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

#[repr(C, align(8))]
struct Word(MaybeUninit<[u8; 8]>);

#[cfg(test)]
mod tests {
    use super::*;

    use std::{cell::Cell, rc::Rc};

    #[test]
    fn drops_owned_values_and_skips_trivial_values() {
        struct Dropped(Rc<Cell<bool>>);

        impl Drop for Dropped {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }

        let dropped = Rc::new(Cell::new(false));
        let mut arena = DataArena::default();
        arena.store(1_u32);
        assert!(arena.drops.is_empty());
        arena.store(Dropped(dropped.clone()));
        assert_eq!(arena.drops.len(), 1);

        arena.clear();
        assert!(dropped.get());
        assert!(arena.drops.is_empty());
    }
}
