use std::{
    hash::{BuildHasher, Hash},
    mem::MaybeUninit,
};

use hashbrown::{hash_map::DefaultHashBuilder, HashTable};

const NONE: usize = usize::MAX;

struct Entry<K, V> {
    key: K,
    value: V,
    weight: usize,
    previous: usize,
    next: usize,
}

pub struct Cache<K, V> {
    table: HashTable<usize>,
    hash_builder: DefaultHashBuilder,
    entries: Vec<MaybeUninit<Entry<K, V>>>,
    free: usize,
    capacity: usize,
    weight: usize,
    first: usize,
    last: usize,
}

impl<K: Copy + Eq + Hash, V> Cache<K, V> {
    pub fn new(capacity: usize) -> Self {
        assert_ne!(capacity, 0, "cache capacity must be non-zero");
        Self {
            table: HashTable::new(),
            hash_builder: DefaultHashBuilder::default(),
            entries: Vec::new(),
            free: NONE,
            capacity,
            weight: 0,
            first: NONE,
            last: NONE,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        let index = self.find(key, self.hash_builder.hash_one(key))?;
        self.promote(index);
        Some(&self.entry(index).value)
    }

    pub fn get_or_insert_with(&mut self, key: K, create: impl FnOnce() -> (V, usize)) -> Result<&V, V> {
        let key_hash = self.hash_builder.hash_one(key);
        if let Some(index) = self.find(&key, key_hash) {
            self.promote(index);
            return Ok(&self.entry(index).value);
        }
        let (value, weight) = create();
        self.insert_missing(key, value, weight, key_hash)
    }

    pub fn pop(&mut self, key: &K) -> Option<V> {
        let key_hash = self.hash_builder.hash_one(key);
        let entries = &self.entries;
        let entry = self
            .table
            .find_entry(key_hash, |index| {
                // safety: table indices always point to initialized entries
                unsafe { entries.get_unchecked(*index).assume_init_ref() }.key == *key
            })
            .ok()?;
        let (index, _) = entry.remove();
        Some(self.take(index).value)
    }

    pub fn insert_unique(&mut self, key: K, value: V, weight: usize) -> Result<&V, V> {
        if weight > self.capacity {
            return Err(value);
        }
        let key_hash = self.hash_builder.hash_one(key);
        debug_assert!(self.find(&key, key_hash).is_none());
        self.insert_missing(key, value, weight, key_hash)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        let mut index = self.first;
        while index != NONE {
            let entry = self.entry(index);
            let next = entry.next;
            if !keep(&entry.key, &entry.value) {
                self.remove(index);
            }
            index = next;
        }
    }

    fn insert_missing(&mut self, key: K, value: V, weight: usize, key_hash: u64) -> Result<&V, V> {
        if weight > self.capacity {
            return Err(value);
        }
        while self.weight.saturating_add(weight) > self.capacity {
            self.remove(self.last);
        }

        let index = if self.free == NONE {
            self.entries.push(MaybeUninit::uninit());
            self.entries.len() - 1
        } else {
            let index = self.free;
            // safety: free slots store the next free index
            self.free = unsafe { self.entries.get_unchecked(index).as_ptr().cast::<usize>().read() };
            index
        };
        if self.first != NONE {
            self.entry_mut(self.first).previous = index;
        } else {
            self.last = index;
        }
        self.entries[index].write(Entry { key, value, weight, previous: NONE, next: self.first });
        self.first = index;
        self.weight += weight;

        let entries = &self.entries;
        let hash_builder = &self.hash_builder;
        self.table.insert_unique(key_hash, index, |index| {
            // safety: table indices always point to initialized entries
            hash_builder.hash_one(unsafe { entries.get_unchecked(*index).assume_init_ref() }.key)
        });
        Ok(&self.entry(index).value)
    }

    fn find(&self, key: &K, hash: u64) -> Option<usize> {
        self.table.find(hash, |index| self.entry(*index).key == *key).copied()
    }

    fn promote(&mut self, index: usize) {
        if index == self.first {
            return;
        }
        let entry = self.entry(index);
        let previous = entry.previous;
        let next = entry.next;
        self.entry_mut(previous).next = next;
        if next == NONE {
            self.last = previous;
        } else {
            self.entry_mut(next).previous = previous;
        }
        self.entry_mut(self.first).previous = index;
        let first = self.first;
        let entry = self.entry_mut(index);
        entry.previous = NONE;
        entry.next = first;
        self.first = index;
    }

    fn remove(&mut self, index: usize) -> Entry<K, V> {
        let entry = self.entry(index);
        let hash = self.hash_builder.hash_one(entry.key);
        self.table
            .find_entry(hash, |candidate| *candidate == index)
            .expect("cache table missing live entry")
            .remove();
        self.take(index)
    }

    fn take(&mut self, index: usize) -> Entry<K, V> {
        let entry = self.entry(index);
        let previous = entry.previous;
        let next = entry.next;
        if previous == NONE {
            self.first = next;
        } else {
            self.entry_mut(previous).next = next;
        }
        if next == NONE {
            self.last = previous;
        } else {
            self.entry_mut(next).previous = previous;
        }

        // safety: live indices point to initialized entries
        let entry = unsafe { self.entries.get_unchecked(index).assume_init_read() };
        self.weight -= entry.weight;
        // safety: every slot is aligned and large enough to store an index
        unsafe { self.entries.get_unchecked_mut(index).as_mut_ptr().cast::<usize>().write(self.free) };
        self.free = index;
        entry
    }

    fn entry(&self, index: usize) -> &Entry<K, V> {
        // safety: callers only pass live indices
        unsafe { self.entries.get_unchecked(index).assume_init_ref() }
    }

    fn entry_mut(&mut self, index: usize) -> &mut Entry<K, V> {
        // safety: callers only pass live indices
        unsafe { self.entries.get_unchecked_mut(index).assume_init_mut() }
    }

    #[cfg(test)]
    fn assert_valid(&self) {
        let mut occupied = vec![false; self.entries.len()];
        let mut index = self.first;
        let mut previous = NONE;
        let mut weight = 0;
        let mut count = 0;
        while index != NONE {
            assert!(index < self.entries.len());
            assert!(!occupied[index]);
            occupied[index] = true;
            let entry = self.entry(index);
            assert_eq!(entry.previous, previous);
            assert_eq!(self.find(&entry.key, self.hash_builder.hash_one(entry.key)), Some(index));
            weight += entry.weight;
            count += 1;
            previous = index;
            index = entry.next;
        }
        assert_eq!(self.last, previous);
        assert_eq!(self.table.len(), count);
        assert_eq!(self.weight, weight);

        index = self.free;
        while index != NONE {
            assert!(index < self.entries.len());
            assert!(!occupied[index]);
            occupied[index] = true;
            // safety: free slots store the next free index
            index = unsafe { self.entries.get_unchecked(index).as_ptr().cast::<usize>().read() };
        }
        assert!(occupied.into_iter().all(|occupied| occupied));
    }
}

impl<K, V> Drop for Cache<K, V> {
    fn drop(&mut self) {
        let mut index = self.first;
        while index != NONE {
            // safety: the lru list contains every initialized entry exactly once
            let next = unsafe { self.entries.get_unchecked(index).assume_init_ref() }.next;
            // safety: each initialized entry is dropped once
            unsafe { self.entries.get_unchecked_mut(index).assume_init_drop() };
            index = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::Cache;

    #[test]
    fn weighted_eviction_and_retain_stay_consistent() {
        let mut cache = Cache::new(10);
        assert_eq!(cache.insert_unique(1, 10, 4), Ok(&10));
        assert_eq!(cache.insert_unique(2, 20, 4), Ok(&20));
        assert_eq!(cache.get(&1), Some(&10));
        assert_eq!(cache.insert_unique(3, 30, 4), Ok(&30));
        assert_eq!(cache.get(&2), None);
        cache.retain(|key, _| *key == 3);
        assert_eq!(cache.insert_unique(1, 11, 6), Ok(&11));
        assert_eq!(cache.get(&3), Some(&30));
        assert_eq!(cache.pop(&1), Some(11));
        assert_eq!(cache.insert_unique(1, 12, 6), Ok(&12));
        cache.assert_valid();
    }

    #[test]
    fn repeated_retain_releases_weight() {
        let mut cache = Cache::new(10);
        for value in 0..1000 {
            assert_eq!(cache.insert_unique(1, value, 4), Ok(&value));
            cache.retain(|_, _| false);
            cache.assert_valid();
        }
        assert_eq!(cache.insert_unique(1, 1000, 10), Ok(&1000));
        cache.assert_valid();
    }

    #[test]
    fn values_drop_once() {
        struct Value(Rc<Cell<usize>>);

        impl Drop for Value {
            fn drop(&mut self) { self.0.set(self.0.get() + 1) }
        }

        let drops = Rc::new(Cell::new(0));
        {
            let mut cache = Cache::new(2);
            let _ = cache.insert_unique(1, Value(drops.clone()), 1);
            let _ = cache.insert_unique(2, Value(drops.clone()), 1);
            let _ = cache.insert_unique(3, Value(drops.clone()), 1);
            cache.retain(|key, _| *key != 2);
            cache.assert_valid();
            assert_eq!(drops.get(), 2);
        }
        assert_eq!(drops.get(), 3);
    }
}
