use super::*;

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct BumpId(u32);

impl BumpId {
    pub const NONE: Self = Self(u32::MAX);

    pub fn offset(self) -> Option<usize> {
        (self.0 != u32::MAX).then_some(self.0 as usize)
    }
}

/// values stored until the frame that created them is rebuilt
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameSlice<T> {
    slice: Slice<T>,
    epoch: u32,
}

#[derive(Default)]
pub struct BumpArena {
    storage: Bump,
    drops: Vec<DropEntry>,
    scratch_used: Cell<usize>,
    scratch_peak: Cell<usize>,
    scratch_live: Cell<usize>,
    // rejects slices from previous frames
    epoch: u32,
}

impl BumpArena {
    pub fn store_slice<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> FrameSlice<T> {
        FrameSlice {
            slice: self.storage.extend(values),
            epoch: self.epoch,
        }
    }

    pub fn slice<T: Copy + 'static>(&self, slice: FrameSlice<T>) -> &[T] {
        if slice.slice.len == 0 {
            return &[];
        }
        assert_eq!(slice.epoch, self.epoch, "frame slice belongs to a previous frame");
        // safety: the epoch proves store_slice wrote these values since the last clear
        unsafe { self.storage.slice(slice.slice) }
    }

    pub fn store<T: 'static>(&mut self, value: T) -> BumpId {
        // reserve before moving value so registering its destructor cannot allocate
        if const { needs_drop::<T>() } {
            self.drops.reserve(1);
        }
        let id = BumpId(self.storage.push(value).byte());
        if const { needs_drop::<T>() } {
            self.drops.push(DropEntry {
                offset: id.0,
                drop: |value| {
                    // safety: the drop entry was created for T at this address
                    unsafe { value.cast::<T>().drop_in_place() };
                },
            });
        }
        assert!(id.0 < u32::MAX, "too much frame data");
        id
    }

    pub fn scratch<T: Copy>(&self, len: usize, value: T) -> Scratch<'_, T> {
        const {
            assert!(
                align_of::<T>() <= Bump::ALIGNMENT,
                "scratch alignment exceeds arena alignment"
            );
        }
        let start = self.scratch_used.get().max(self.storage.used());
        let offset = start.next_multiple_of(align_of::<T>());
        let end = size_of::<T>()
            .checked_mul(len)
            .and_then(|bytes| offset.checked_add(bytes))
            .filter(|&end| end <= isize::MAX as usize)
            .expect("too much scratch data");
        let live = self.scratch_live.get().checked_add(1).expect("too many scratch guards");
        let inner = if end <= self.storage.capacity() {
            let pointer = if len == 0 || size_of::<T>() == 0 {
                std::ptr::NonNull::<T>::dangling().as_ptr()
            } else {
                // safety: this range is disjoint from retained data and all live scratch guards
                unsafe { self.storage.as_ptr().add(offset).cast::<T>() }
            };
            // safety: the storage is aligned and sized for this slice
            let values = unsafe { std::slice::from_raw_parts_mut(pointer.cast::<MaybeUninit<T>>(), len) };
            values.fill(MaybeUninit::new(value));
            // safety: every element has been initialized
            ScratchInner::Ref(unsafe { values.assume_init_mut() })
        } else {
            ScratchInner::Owned(vec![value; len])
        };
        self.scratch_used.set(end);
        self.scratch_peak.set(self.scratch_peak.get().max(end));
        self.scratch_live.set(live);
        Scratch {
            inner,
            bump: self,
            start,
            end,
        }
    }

    pub fn prepare_scratch(&mut self) {
        self.storage.reserve(self.scratch_peak.get());
        self.scratch_used.set(self.storage.used());
        self.scratch_live.set(0);
    }

    pub fn load<T: 'static>(&self, id: BumpId) -> &T {
        // safety: store wrote T at this checked address
        unsafe { &*self.pointer::<T>(id) }
    }

    pub fn load_mut<T: 'static>(&mut self, id: BumpId) -> &mut T {
        // safety: the mutable arena borrow makes this checked address exclusive
        unsafe { &mut *self.pointer::<T>(id) }
    }

    pub fn clear(&mut self) {
        let pointer = self.storage.as_ptr();
        while let Some(entry) = self.drops.pop() {
            // safety: entries point to initialized values in the arena
            (entry.drop)(unsafe { pointer.add(entry.offset as usize) });
        }
        self.storage.clear();
        self.epoch = self.epoch.wrapping_add(1);
    }
}

impl BumpArena {
    fn pointer<T>(&self, id: BumpId) -> *mut T {
        let offset = id.offset().expect("frame data is missing");
        assert!(
            offset
                .checked_add(size_of::<T>())
                .is_some_and(|end| end <= self.storage.used())
        );
        if size_of::<T>() == 0 {
            return std::ptr::NonNull::<T>::dangling().as_ptr();
        }
        // safety: the offset is within the arena and shared writes use disjoint reserved ranges
        unsafe { self.storage.as_ptr().add(offset).cast::<T>() }
    }
}

impl Drop for BumpArena {
    fn drop(&mut self) {
        self.clear();
    }
}

/// temporary slice reclaimed when its guard drops
pub struct Scratch<'a, T> {
    inner: ScratchInner<'a, T>,
    bump: &'a BumpArena,
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
        let live = self.bump.scratch_live.get() - 1;
        self.bump.scratch_live.set(live);
        // gaps from guards dropped out of order remain until the last guard drops
        if live == 0 {
            self.bump.scratch_used.set(self.bump.storage.used());
        } else if self.bump.scratch_used.get() == self.end {
            self.bump.scratch_used.set(self.start);
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
