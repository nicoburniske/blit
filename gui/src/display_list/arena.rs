use std::{
    any::TypeId,
    marker::PhantomData,
    mem::needs_drop,
    sync::atomic::{AtomicU64, Ordering},
};

use blit::internal::bump::{Bump, Offset};

#[derive(Default)]
pub struct Arena {
    pub(crate) bump: Bump,
    drops: Vec<(u32, unsafe fn(&mut Bump, u32))>,
    owner: u64,
}

pub struct Writer<'a>(&'a mut Arena);

/// typed handles are invariant and tied to their arena epoch
///
/// ```compile_fail
/// use blit_gui::display_list::arena::{Arena, Handle};
/// let mut arena = Arena::default();
/// let handle: Handle<fn(&str)> = arena.push((|_: &str| {}) as fn(&str));
/// let _: Handle<fn(&'static str)> = handle;
/// ```
pub struct Handle<T> {
    pub(crate) offset: u32,
    owner: u64,
    marker: PhantomData<fn(T) -> T>,
}

pub struct Slice<T> {
    handle: Handle<T>,
    len: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Value {
    offset: u32,
    owner: u64,
    kind: TypeId,
}

impl Arena {
    pub fn writer(&mut self) -> Writer<'_> {
        Writer(self)
    }

    pub fn push<T: 'static>(&mut self, value: T) -> Handle<T> {
        self.initialize();
        if needs_drop::<T>() {
            self.drops.reserve(1);
        }
        let offset = self.bump.push(value).byte();
        if needs_drop::<T>() {
            unsafe fn drop_value<T>(bump: &mut Bump, offset: u32) {
                // safety: the arena records the type and removes this entry before dropping it
                unsafe { std::ptr::drop_in_place(bump.get_mut(Offset::<T>::new(offset))) };
            }
            self.drops.push((offset, drop_value::<T>));
        }
        Handle {
            offset,
            owner: self.owner,
            marker: PhantomData,
        }
    }

    pub fn extend<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> Slice<T> {
        self.initialize();
        let slice = self.bump.extend(values);
        Slice {
            handle: Handle {
                offset: slice.offset.byte(),
                owner: self.owner,
                marker: PhantomData,
            },
            len: slice.len,
        }
    }

    pub fn get<T: 'static>(&self, handle: Handle<T>) -> &T {
        assert_eq!(handle.owner, self.owner, "arena value belongs to another frame");
        // safety: private handles preserve the inserted type and are checked against this arena epoch
        unsafe { self.bump.get(Offset::new(handle.offset)) }
    }

    pub fn value<T: 'static>(&self, value: Value) -> Option<&T> {
        assert_eq!(value.owner, self.owner, "arena value belongs to another frame");
        if value.kind != TypeId::of::<T>() {
            return None;
        }
        // safety: private handles preserve the inserted type and are checked against this arena epoch
        Some(unsafe { self.bump.get(Offset::new(value.offset)) })
    }

    pub fn slice<T: Copy + 'static>(&self, slice: Slice<T>) -> &[T] {
        assert_eq!(slice.handle.owner, self.owner, "arena slice belongs to another frame");
        // safety: the private handle preserves the inserted type length and arena epoch
        unsafe {
            self.bump.slice(blit::internal::bump::Slice {
                offset: Offset::new(slice.handle.offset),
                len: slice.len,
            })
        }
    }

    pub fn clear(&mut self) {
        self.owner = 0;
        while let Some((offset, drop_value)) = self.drops.pop() {
            // safety: each entry identifies one live value and is removed before its destructor runs
            unsafe { drop_value(&mut self.bump, offset) };
        }
        self.bump.clear();
    }

    pub fn used(&self) -> usize {
        self.bump.used()
    }

    fn initialize(&mut self) {
        static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);
        if self.owner == 0 {
            self.owner = NEXT_OWNER
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |owner| owner.checked_add(1))
                .expect("arena epochs exhausted");
        }
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        self.clear();
    }
}

impl Writer<'_> {
    pub fn push<T: 'static>(&mut self, value: T) -> Handle<T> {
        self.0.push(value)
    }

    pub fn extend<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> Slice<T> {
        self.0.extend(values)
    }
}

impl<T: 'static> Handle<T> {
    pub fn erase(self) -> Value {
        Value {
            offset: self.offset,
            owner: self.owner,
            kind: TypeId::of::<T>(),
        }
    }
}

impl<T> Copy for Handle<T> {}
impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Slice<T> {}
impl<T> Clone for Slice<T> {
    fn clone(&self) -> Self {
        *self
    }
}
