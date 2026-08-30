use std::mem::{MaybeUninit, align_of, size_of};

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
    len: usize,
}

impl DataArena {
    pub fn store<T: Copy>(&mut self, value: T) -> DataId {
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
        self.words.resize_with(end.div_ceil(size_of::<Word>()), || {
            Word(MaybeUninit::uninit())
        });
        // safety: the backing words are aligned and sized for T
        unsafe {
            self.words
                .as_mut_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<T>()
                .write(value)
        };
        self.len = end;
        DataId(u32::try_from(offset).expect("too much frame data"))
    }

    pub fn load<T: Copy>(&self, id: DataId) -> T {
        let offset = id.offset().expect("frame data is missing");
        assert!(align_of::<T>() <= 8);
        assert_eq!(offset % align_of::<T>(), 0);
        assert!(
            offset
                .checked_add(size_of::<T>())
                .is_some_and(|end| end <= self.len)
        );
        // safety: store wrote an aligned T at this offset
        unsafe {
            self.words
                .as_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<T>()
                .read()
        }
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn heap_bytes(&self) -> usize {
        self.words.capacity() * size_of::<Word>()
    }
}

#[repr(C, align(8))]
struct Word(MaybeUninit<[u8; 8]>);
