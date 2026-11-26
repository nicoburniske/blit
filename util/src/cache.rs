//! weighted SIEVE cache with deferred weight trimming

use alloc::vec::Vec;
use core::{
    cell::Cell,
    hash::{BuildHasher, Hash},
};

pub use hashbrown::Equivalent;

const NONE: u32 = u32::MAX;

pub trait Scale<K, V> {
    fn weight(&self, key: &K, value: &V) -> usize;
}

pub struct DeferredCache<K, V, S: Scale<K, V>> {
    hash_builder: hashbrown::DefaultHashBuilder,
    table: hashbrown::HashTable<u32>,
    slots: Vec<Slot<K, V>>,
    head: u32,
    tail: u32,
    hand: u32,
    free_head: u32,
    scale: S,
    weight: usize,
    max_weight: usize,
}

enum Slot<K, V> {
    Occupied(Entry<K, V>),
    Vacant { next_free: u32 },
}

struct Entry<K, V> {
    key: K,
    value: V,
    hash: u64,
    weight: usize,
    // prev points toward head (newest), next points toward tail (oldest)
    prev: u32,
    next: u32,
    visited: Cell<bool>,
}

impl<K, V, S> DeferredCache<K, V, S>
where
    K: Hash,
    S: Scale<K, V>,
{
    /// creates an empty cache with a target maximum weight
    pub fn new(scale: S, max_weight: usize) -> Self {
        Self {
            hash_builder: Default::default(),
            table: hashbrown::HashTable::new(),
            slots: Vec::new(),
            head: NONE,
            tail: NONE,
            hand: NONE,
            free_head: NONE,
            scale,
            weight: 0,
            max_weight,
        }
    }

    /// returns the matching entry and marks it visited
    #[inline]
    pub fn get<Q>(&self, query: &Q) -> Option<(&V, usize)>
    where
        Q: Equivalent<K> + Hash + ?Sized,
    {
        let hash = self.hash_builder.hash_one(query);
        let &index = self
            .table
            .find(hash, |&index| query.equivalent(&self.entry(index as usize).key))?;
        let entry = self.entry(index as usize);
        if !entry.visited.get() {
            entry.visited.set(true);
        }
        Some((&entry.value, index as usize))
    }

    /// returns the existing entry or inserts a new one, marking hits as visited
    pub fn get_or_insert<Q>(&mut self, query: Q, insert: impl FnOnce(Q) -> (K, V)) -> (&V, usize)
    where
        Q: Equivalent<K> + Hash,
    {
        self.try_get_or_insert(query, |query| Some(insert(query)))
            .expect("infallible insert returned none")
    }

    /// returns the existing entry or tries to insert a new one, marking hits as visited
    pub fn try_get_or_insert<Q>(&mut self, query: Q, insert: impl FnOnce(Q) -> Option<(K, V)>) -> Option<(&V, usize)>
    where
        Q: Equivalent<K> + Hash,
    {
        let hash = self.hash_builder.hash_one(&query);
        if let Some(&index) = self
            .table
            .find(hash, |&index| query.equivalent(&self.entry(index as usize).key))
        {
            let entry = self.entry(index as usize);
            if !entry.visited.get() {
                entry.visited.set(true);
            }
            return Some((&entry.value, index as usize));
        }
        let (key, value) = insert(query)?;
        debug_assert_eq!(hash, self.hash_builder.hash_one(&key));
        let weight = self.scale.weight(&key, &value);
        let entry = Entry {
            key,
            value,
            hash,
            weight,
            prev: NONE,
            next: self.head,
            visited: Cell::new(false),
        };
        let index = if self.free_head != NONE {
            let index = self.free_head;
            let Slot::Vacant { next_free } = self.slots[index as usize] else {
                panic!("cache free list points to occupied slot");
            };
            self.free_head = next_free;
            self.slots[index as usize] = Slot::Occupied(entry);
            index
        } else {
            assert!(self.slots.len() < NONE as usize, "too many cache entries");
            let index = self.slots.len() as u32;
            self.slots.push(Slot::Occupied(entry));
            index
        };
        if self.head != NONE {
            self.entry_mut(self.head as usize).prev = index;
        } else {
            self.tail = index;
        }
        self.head = index;
        let slots = &self.slots;
        self.table.insert_unique(hash, index, |&existing| match &slots[existing as usize] {
            Slot::Occupied(entry) => entry.hash,
            Slot::Vacant { .. } => panic!("cache table points to vacant slot"),
        });
        self.weight += weight;
        Some((&self.entry(index as usize).value, index as usize))
    }

    /// returns the key at `index` without marking it visited
    #[inline]
    pub fn get_key_index(&self, index: usize) -> &K {
        &self.entry(index).key
    }

    /// returns the value at `index` without marking it visited
    #[inline]
    pub fn get_index(&self, index: usize) -> &V {
        &self.entry(index).value
    }

    /// returns the value at `index` if occupied without marking it visited
    #[inline]
    pub fn try_get_index(&self, index: usize) -> Option<&V> {
        match self.slots.get(index)? {
            Slot::Occupied(entry) => Some(&entry.value),
            Slot::Vacant { .. } => None,
        }
    }

    /// returns mutable access to the value at `index` without marking it visited or reweighing
    #[inline]
    pub fn get_mut_index(&mut self, index: usize) -> &mut V {
        &mut self.entry_mut(index).value
    }

    /// marks the entry at `index` as visited
    #[inline]
    pub fn touch_index(&self, index: usize) {
        let entry = self.entry(index);
        if !entry.visited.get() {
            entry.visited.set(true);
        }
    }

    /// updates the value at `index` and recalculates its tracked weight
    pub fn update_index<R>(&mut self, index: usize, update: impl FnOnce(&mut V) -> R) -> R {
        let result = update(&mut self.entry_mut(index).value);
        let new_weight = {
            let entry = self.entry(index);
            self.scale.weight(&entry.key, &entry.value)
        };
        let entry = self.entry_mut(index);
        let old_weight = entry.weight;
        entry.weight = new_weight;
        self.weight = self.weight - old_weight + new_weight;
        result
    }

    /// evicts unvisited entries until total weight fits within the budget
    pub fn trim_to_weight(&mut self) {
        self.trim_to_weight_if(|_, _| true);
    }

    /// evicts unvisited entries accepted by `removable` until total weight fits within the budget
    pub fn trim_to_weight_if(&mut self, mut removable: impl FnMut(&K, &V) -> bool) {
        let mut wraps = 0u8;
        while self.weight > self.max_weight && self.tail != NONE {
            if self.hand == NONE {
                if wraps == 2 {
                    break;
                }
                self.hand = self.tail;
                wraps += 1;
            }
            let index = self.hand;
            let (prev, evict) = {
                let entry = self.entry(index as usize);
                let prev = entry.prev;
                if entry.visited.get() {
                    entry.visited.set(false);
                    (prev, false)
                } else {
                    (prev, removable(&entry.key, &entry.value))
                }
            };
            if !evict {
                self.hand = prev;
                continue;
            }
            let Slot::Occupied(removed) = core::mem::replace(
                &mut self.slots[index as usize],
                Slot::Vacant {
                    next_free: self.free_head,
                },
            ) else {
                panic!("cache slot is vacant");
            };
            self.free_head = index;
            self.table
                .find_entry(removed.hash, |&candidate| candidate == index)
                .expect("cache table missing live entry")
                .remove();
            self.weight -= removed.weight;
            self.hand = removed.prev;
            if self.head == index {
                self.head = removed.next;
            }
            if self.tail == index {
                self.tail = removed.prev;
            }
            if removed.prev != NONE {
                self.entry_mut(removed.prev as usize).next = removed.next;
            }
            if removed.next != NONE {
                self.entry_mut(removed.next as usize).prev = removed.prev;
            }
        }
    }

    #[inline]
    fn entry(&self, index: usize) -> &Entry<K, V> {
        let Slot::Occupied(entry) = &self.slots[index] else {
            panic!("cache slot is vacant");
        };
        entry
    }

    #[inline]
    fn entry_mut(&mut self, index: usize) -> &mut Entry<K, V> {
        let Slot::Occupied(entry) = &mut self.slots[index] else {
            panic!("cache slot is vacant");
        };
        entry
    }
}

#[cfg(test)]
mod test {
    use super::*;

    struct UnitWeight;

    impl Scale<u32, u32> for UnitWeight {
        fn weight(&self, _key: &u32, _value: &u32) -> usize {
            1
        }
    }

    struct ValueWeight;

    impl Scale<u32, u32> for ValueWeight {
        fn weight(&self, _key: &u32, value: &u32) -> usize {
            *value as usize
        }
    }

    #[test]
    fn deferred_eviction() {
        let mut cache = DeferredCache::new(UnitWeight, 2);

        let (value, index) = cache.get_or_insert(1, |key| (key, 10));
        assert_eq!((*value, index), (10, 0));
        cache.get_or_insert(2, |key| (key, 20));
        cache.get_or_insert(3, |key| (key, 30));

        assert_eq!(cache.table.len(), 3);
        assert_eq!(cache.weight, 3);

        cache.trim_to_weight();

        assert_eq!(cache.get(&1).map(|(value, _)| value), None);
        assert_eq!(cache.get(&2).map(|(value, _)| value), Some(&20));
        assert_eq!(cache.get(&3).map(|(value, _)| value), Some(&30));
        assert_eq!(cache.weight, 2);
    }

    #[test]
    fn index_access_updates_weight() {
        let mut cache = DeferredCache::new(ValueWeight, 2);

        let (_, first) = cache.get_or_insert(1, |key| (key, 1));
        cache.get_or_insert(2, |key| (key, 1));
        cache.touch_index(first);
        cache.update_index(first, |value| *value = 2);

        assert_eq!(cache.get_index(first), &2);
        assert_eq!(cache.weight, 3);

        cache.trim_to_weight();

        assert_eq!(cache.get(&1).map(|(value, _)| value), Some(&2));
        assert_eq!(cache.get(&2).map(|(value, _)| value), None);
    }

    #[test]
    fn pinned_entries_terminate_and_skip() {
        let mut cache = DeferredCache::new(UnitWeight, 2);
        for key in 0..5u32 {
            cache.get_or_insert(key, |k| (k, k * 10));
            cache.get(&key);
        }
        cache.trim_to_weight_if(|&key, _| key >= 3);
        assert_eq!(cache.weight, 3);
        assert!(cache.get(&0).is_some());
        assert!(cache.get(&1).is_some());
        assert!(cache.get(&2).is_some());
        assert!(cache.get(&3).is_none());
        assert!(cache.get(&4).is_none());
    }
}
