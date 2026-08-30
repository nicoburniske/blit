use std::mem::{MaybeUninit, align_of, size_of};

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct DataId(pub u32);

#[derive(Default)]
pub struct DataArena {
    words: Vec<Word>,
}

impl DataArena {
    pub fn store<T: Copy>(&mut self, value: T) -> DataId {
        const {
            assert!(align_of::<T>() <= 8, "frame data alignment exceeds 8 bytes");
        }
        let start = self.words.len();
        let words = size_of::<T>().div_ceil(8);
        self.words
            .resize_with(start + words, || Word(MaybeUninit::uninit()));
        // safety: each allocation begins at an 8-byte-aligned word
        unsafe { self.words.as_mut_ptr().add(start).cast::<T>().write(value) };
        DataId(u32::try_from(start).expect("too much frame data"))
    }

    pub fn load<T: Copy>(&self, id: DataId) -> T {
        let start = id.0 as usize;
        let words = size_of::<T>().div_ceil(8);
        assert!(start + words <= self.words.len());
        assert!(align_of::<T>() <= 8);
        // safety: the owning record uses the same type that was stored at id
        unsafe { self.words.as_ptr().add(start).cast::<T>().read() }
    }

    pub fn clear(&mut self) {
        self.words.clear();
    }
}

#[repr(C, align(8))]
struct Word(MaybeUninit<[u8; 8]>);
