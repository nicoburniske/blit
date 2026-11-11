use std::sync::atomic::{AtomicU32, Ordering};

use crate::internal::bump::{Bump, Slice};

/// drawing inputs owned by a rendering context until it clears them
#[derive(Default)]
pub struct FrameData {
    data: Bump,
    epoch: u32,
}

/// a typed slice resolved through the FrameData that allocated it
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameSlice<T> {
    slice: Slice<T>,
    epoch: u32,
}

impl FrameData {
    /// stores drawing inputs until this storage is cleared
    pub fn store_slice<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> FrameSlice<T> {
        static NEXT_EPOCH: AtomicU32 = AtomicU32::new(1);
        if self.epoch == 0 {
            self.epoch = NEXT_EPOCH
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |epoch| epoch.checked_add(1))
                .expect("frame data epochs exhausted");
        }
        FrameSlice {
            slice: self.data.extend(values),
            epoch: self.epoch,
        }
    }

    pub fn slice<T: Copy + 'static>(&self, slice: FrameSlice<T>) -> &[T] {
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

    /// clears inputs while keeping the allocation for reuse
    pub fn clear(&mut self) {
        self.epoch = 0;
        self.data.clear();
    }
}
