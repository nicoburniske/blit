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

#[derive(Default)]
pub struct BumpArena {
    pub storage: ScratchBuffer,
}

impl BumpArena {
    pub fn store<T: 'static>(&mut self, value: T) -> BumpId {
        let id = BumpId(self.storage.bump.push(value).byte());
        assert!(id.0 < u32::MAX, "too much frame data");
        id
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
        self.storage.bump.clear();
    }
}

impl BumpArena {
    fn pointer<T>(&self, id: BumpId) -> *mut T {
        let offset = id.offset().expect("frame data is missing");
        assert!(
            offset
                .checked_add(size_of::<T>())
                .is_some_and(|end| end <= self.storage.bump.used())
        );
        if size_of::<T>() == 0 {
            return std::ptr::NonNull::<T>::dangling().as_ptr();
        }
        // safety: the offset is within the arena and shared writes use disjoint reserved ranges
        unsafe { self.storage.bump.as_ptr().add(offset).cast::<T>() }
    }
}
