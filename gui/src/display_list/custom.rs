use std::{
    any::{TypeId, type_name},
    fmt,
    hash::{Hash, Hasher},
};

use blit::internal::bump::Offset;

use super::arena::Arena;

pub trait CustomCommand: 'static {
    type View<'a>: PartialEq + Hash;

    fn view<'a>(&'a self, arena: &'a Arena) -> Self::View<'a>;
}

impl<T: PartialEq + Hash + 'static> CustomCommand for T {
    type View<'a> = &'a T;

    fn view<'a>(&'a self, _arena: &'a Arena) -> Self::View<'a> {
        self
    }
}

#[derive(Clone, Copy)]
pub struct Custom<'a> {
    pub(crate) arena: &'a Arena,
    pub(crate) offset: Offset<Header>,
    pub(crate) table: &'static Table,
}

pub struct Header {
    pub table: &'static Table,
}

#[repr(C)]
pub struct StoredCustom<T> {
    pub header: Header,
    pub value: T,
}

impl<T: CustomCommand> StoredCustom<T> {
    pub const TABLE: Table = Table {
        kind: TypeId::of::<T>(),
        name: type_name::<T>,
        hash: |arena, offset, mut state| {
            // safety: this table is attached only to a live stored value of this type
            unsafe { arena.bump.get::<Self>(Offset::new(offset)) }
                .value
                .view(arena)
                .hash(&mut state)
        },
        equivalent: |left, a, right, b| {
            // safety: matching tables establish the type and both offsets belong to live commands
            let (left_value, right_value) = unsafe {
                (
                    left.bump.get::<Self>(Offset::new(a)),
                    right.bump.get::<Self>(Offset::new(b)),
                )
            };
            left_value.value.view(left) == right_value.value.view(right)
        },
    };
}

impl<'a> Custom<'a> {
    #[inline]
    pub fn get<T: CustomCommand>(self) -> Option<T::View<'a>> {
        if self.type_id() != TypeId::of::<T>() {
            return None;
        }
        // safety: matching the private table establishes the type and the record borrows this arena
        let value = unsafe { self.arena.bump.get::<StoredCustom<T>>(Offset::new(self.offset.byte())) };
        Some(value.value.view(self.arena))
    }

    pub fn type_id(self) -> TypeId {
        self.table.kind
    }
    pub fn type_name(self) -> &'static str {
        (self.table.name)()
    }
}

impl Hash for Custom<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_id().hash(state);
        (self.table.hash)(self.arena, self.offset.byte(), state);
    }
}

impl PartialEq for Custom<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.type_id() == other.type_id()
            && (self.table.equivalent)(self.arena, self.offset.byte(), other.arena, other.offset.byte())
    }
}

impl fmt::Debug for Custom<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Custom").field(&self.type_name()).finish()
    }
}

pub struct Table {
    kind: TypeId,
    name: fn() -> &'static str,
    hash: fn(&Arena, u32, &mut dyn Hasher),
    equivalent: fn(&Arena, u32, &Arena, u32) -> bool,
}
