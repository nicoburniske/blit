use std::sync::atomic::{AtomicU32, Ordering};

use blit::internal::bump::{Bump, Offset};

#[derive(Default)]
pub struct Arena {
    storage: Bump,
    epoch: u32,
}

/// slices stored until their display list is cleared
#[derive(Clone, Copy)]
pub struct Slice<T> {
    slice: blit::internal::bump::Slice<T>,
    epoch: u32,
}

impl Arena {
    pub fn slice<T: Copy + 'static>(&self, slice: Slice<T>) -> &[T] {
        assert_eq!(
            slice.epoch, self.epoch,
            "slice belongs to another display list or frame"
        );
        // safety: the private slice preserves its type and the epoch identifies its storage
        unsafe { self.storage.slice(slice.slice) }
    }

    /// # Safety
    /// the offset must identify a live initialized T in this storage
    pub unsafe fn load<T: 'static>(&self, offset: u32) -> &T {
        // safety: the caller pairs the offset with its stored type
        unsafe { self.storage.get(Offset::new(offset)) }
    }

    pub(crate) fn store<T: 'static>(&mut self, value: T) -> u32 {
        self.storage.push(value).byte()
    }

    pub(crate) fn store_slice<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> Slice<T> {
        static NEXT_EPOCH: AtomicU32 = AtomicU32::new(1);
        if self.epoch == 0 {
            self.epoch = NEXT_EPOCH
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |epoch| epoch.checked_add(1))
                .expect("display list epochs exhausted");
        }
        Slice {
            slice: self.storage.extend(values),
            epoch: self.epoch,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.epoch = 0;
        self.storage.clear();
    }
}
