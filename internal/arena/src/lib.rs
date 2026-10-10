use std::{fmt, marker::PhantomData, mem::ManuallyDrop};

pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free_head: u32,
    len: usize,
}

impl<T> Arena<T> {
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_head: 0,
            len: 0,
        }
    }

    pub fn insert(&mut self, value: T) -> Index<T> {
        self.insert_with(|_| value)
    }

    pub fn insert_with(&mut self, create: impl FnOnce(Index<T>) -> T) -> Index<T> {
        if let Some(slot) = self.slots.get_mut(self.free_head as usize) {
            let generation = slot.generation | 1;
            let id = Index::from((u64::from(generation) << 32) | u64::from(self.free_head));
            let value = create(id);
            // the free list contains only vacant slots
            self.free_head = unsafe { slot.data.next_free };
            slot.data.value = ManuallyDrop::new(value);
            slot.generation = generation;
            self.len += 1;
            return id;
        }

        assert!(self.slots.len() < u32::MAX as usize, "too many arena values");
        let id = Index::from((1_u64 << 32) | self.slots.len() as u64);
        self.slots.push(Slot {
            generation: 1,
            data: SlotData {
                value: ManuallyDrop::new(create(id)),
            },
        });
        self.free_head = self.slots.len() as u32;
        self.len += 1;
        id
    }

    pub fn get(&self, id: Index<T>) -> Option<&T> {
        let slot = self.slots.get(id.0 as u32 as usize)?;
        if slot.generation & 1 == 0 || slot.generation != (id.0 >> 32) as u32 {
            return None;
        }
        // odd generations contain an initialized value
        Some(unsafe { &slot.data.value })
    }

    pub fn get_mut(&mut self, id: Index<T>) -> Option<&mut T> {
        let slot = self.slots.get_mut(id.0 as u32 as usize)?;
        if slot.generation & 1 == 0 || slot.generation != (id.0 >> 32) as u32 {
            return None;
        }
        // odd generations contain an initialized value
        Some(unsafe { &mut slot.data.value })
    }

    pub fn remove(&mut self, id: Index<T>) -> Option<T> {
        let index = id.0 as u32;
        let slot = self.slots.get_mut(index as usize)?;
        if slot.generation & 1 == 0 || slot.generation != (id.0 >> 32) as u32 {
            return None;
        }
        // move the occupied value out before linking the vacant slot
        let value = unsafe { ManuallyDrop::take(&mut slot.data.value) };
        self.len -= 1;
        slot.generation = slot.generation.wrapping_add(1);
        slot.data.next_free = self.free_head;
        self.free_head = index;
        Some(value)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&mut T) -> bool) {
        for index in 0..self.slots.len() {
            let slot = &mut self.slots[index];
            if slot.generation & 1 != 0
                // odd generations contain an initialized value
                && !keep(unsafe { &mut slot.data.value })
            {
                let id = (u64::from(slot.generation) << 32) | index as u64;
                self.remove(id.into());
            }
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[repr(transparent)]
pub struct Index<T>(u64, PhantomData<fn() -> T>);

impl<T> From<u64> for Index<T> {
    fn from(value: u64) -> Self {
        Self(value, PhantomData)
    }
}

impl<T> From<Index<T>> for u64 {
    fn from(index: Index<T>) -> Self {
        index.0
    }
}

impl<T> Copy for Index<T> {}

impl<T> Clone for Index<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> PartialEq for Index<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<T> Eq for Index<T> {}

impl<T> fmt::Debug for Index<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("Index").field(&self.0).finish()
    }
}

struct Slot<T> {
    generation: u32,
    data: SlotData<T>,
}

union SlotData<T> {
    value: ManuallyDrop<T>,
    next_free: u32,
}

impl<T> Drop for Slot<T> {
    fn drop(&mut self) {
        if self.generation & 1 != 0 {
            // vacant slots have already had their value removed
            unsafe { ManuallyDrop::drop(&mut self.data.value) };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::*;

    #[test]
    fn reused_slots_reject_stale_ids() {
        let mut arena = Arena::new();
        let first = arena.insert(10);
        let second = arena.insert(20);
        assert_eq!(arena.remove(first), Some(10));
        let replacement = arena.insert_with(|id| {
            assert_ne!(id, first);
            assert_eq!(u64::from(id) as u32, u64::from(first) as u32);
            30
        });
        assert_eq!(arena.get(0.into()), None);
        assert_eq!(arena.get(u64::MAX.into()), None);
        assert_eq!(arena.get(first), None);
        assert_eq!(arena.get_mut(first), None);
        assert_eq!(arena.remove(first), None);
        *arena.get_mut(second).unwrap() += 1;
        arena.retain(|value| *value != 21);
        assert_eq!(arena.get(second), None);
        assert_eq!(arena.get(replacement), Some(&30));
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.remove(replacement), Some(30));
        assert!(arena.is_empty());
        let vacant = Index::from(u64::from(replacement) + (1_u64 << 32));
        assert_eq!(arena.get(vacant), None);
        assert_eq!(arena.get_mut(vacant), None);
        assert_eq!(arena.remove(vacant), None);
        let third = arena.insert(40);
        let fourth = arena.insert(50);
        assert_eq!(u64::from(third) as u32, u64::from(first) as u32);
        assert_eq!(u64::from(fourth) as u32, u64::from(second) as u32);
        assert_eq!(arena.get(third), Some(&40));
        assert_eq!(arena.get(fourth), Some(&50));
    }

    #[test]
    fn panicking_insert_preserves_free_slot() {
        let mut arena = Arena::new();
        let first = arena.insert(10);
        arena.remove(first);
        assert!(catch_unwind(AssertUnwindSafe(|| arena.insert_with(|_| panic!("failed")))).is_err());
        assert!(arena.is_empty());
        let replacement = arena.insert(20);
        assert_eq!(u64::from(replacement) as u32, u64::from(first) as u32);
        assert_eq!(arena.get(replacement), Some(&20));
        assert_eq!(arena.get(first), None);
    }

    #[test]
    fn exhausted_generation_wraps_and_reuses_slot() {
        let mut arena = Arena::new();
        let first = arena.insert(10);
        arena.slots[u64::from(first) as u32 as usize].generation = u32::MAX;
        let last = Index::from((u64::from(u32::MAX) << 32) | (u64::from(first) & u64::from(u32::MAX)));
        assert_eq!(arena.remove(last), Some(10));
        let replacement = arena.insert(20);
        assert_eq!(u64::from(replacement) as u32, u64::from(first) as u32);
        assert_eq!(u64::from(replacement) >> 32, 1);
        assert_eq!(replacement, first);
        assert_eq!(arena.get(last), None);
        assert_eq!(arena.get(replacement), Some(&20));
    }

    #[test]
    fn values_are_dropped_once() {
        use std::cell::Cell;

        struct Value<'a>(&'a Cell<usize>);

        impl Drop for Value<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let drops = Cell::new(0);
        {
            let mut arena = Arena::new();
            let first = arena.insert(Value(&drops));
            arena.insert(Value(&drops));
            let removed = arena.remove(first).unwrap();
            assert_eq!(drops.get(), 0);
            drop(removed);
            assert_eq!(drops.get(), 1);
            arena.insert(Value(&drops));
            arena.retain(|_| false);
            assert_eq!(drops.get(), 3);
            arena.insert(Value(&drops));
        }
        assert_eq!(drops.get(), 4);
    }
}
