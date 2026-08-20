//! weighted lru cache
//!
//! can have deferred weight trimming (wait until end of frame)

mod list;

use std::hash::BuildHasher;

pub trait Scale<K, V> {
    fn weight(&self, key: &K, value: &V) -> usize;
}

pub type Cache<K, V, S> = LruCache<K, V, S, true>;
pub type DeferredCache<K, V, S> = LruCache<K, V, S, false>;

pub struct LruCache<K, V, S: Scale<K, V>, const TRIM_ON_INSERT: bool> {
    hash_builder: hashbrown::hash_map::DefaultHashBuilder,
    table: hashbrown::HashTable<usize>,
    entries: list::LruList<Entry<K, V>>,
    scale: S,
    weight: usize,
    max_weight: usize,
}

struct Entry<K, V> {
    key: K,
    value: V,
}

impl<K, V, S, const TRIM_ON_INSERT: bool> LruCache<K, V, S, TRIM_ON_INSERT>
where
    K: std::hash::Hash + PartialEq,
    S: Scale<K, V>,
{
    pub fn new(scale: S, max_weight: usize) -> Self {
        Self {
            hash_builder: Default::default(),
            table: Default::default(),
            entries: list::LruList::<Entry<K, V>>::new(),
            scale,
            weight: 0,
            max_weight,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        let hash = self.hash_builder.hash_one(key);
        let index = self
            .table
            .find(hash, |index| &self.entries.get(*index).key == key)
            .copied()?;
        self.entries.promote(index);
        Some(&self.entries.get(index).value)
    }

    pub fn get_or_insert_by<Q>(
        &mut self,
        query: &Q,
        equivalent: impl Fn(&K, &V) -> bool,
        insert: impl FnOnce() -> (K, V),
    ) -> Result<(&V, usize), V>
    where
        Q: std::hash::Hash + ?Sized,
    {
        let hash = self.hash_builder.hash_one(query);
        if let Some(index) = self
            .table
            .find(hash, |index| {
                let entry = self.entries.get(*index);
                equivalent(&entry.key, &entry.value)
            })
            .copied()
        {
            self.entries.promote(index);
            return Ok((&self.entries.get(index).value, index));
        }

        let (key, value) = insert();
        debug_assert_eq!(hash, self.hash_builder.hash_one(&key));
        let weight = self.scale.weight(&key, &value);
        if TRIM_ON_INSERT && weight > self.max_weight {
            return Err(value);
        }
        let (_, index) = self.entries.insert(Entry { key, value });
        let entries = &self.entries;
        let hash_builder = &self.hash_builder;
        self.table.insert_unique(hash, index, |index| {
            hash_builder.hash_one(&entries.get(*index).key)
        });
        self.weight += weight;
        if TRIM_ON_INSERT {
            self.trim_to_weight();
        }
        Ok((&self.entries.get(index).value, index))
    }

    pub fn get_index(&self, index: usize) -> &V {
        &self.entries.get(index).value
    }

    pub fn update_index<R>(&mut self, index: usize, update: impl FnOnce(&mut V) -> R) -> R {
        let entry = self.entries.get(index);
        let old_weight = self.scale.weight(&entry.key, &entry.value);
        self.entries.promote(index);
        let result = update(&mut self.entries.get_mut(index).value);
        let entry = self.entries.get(index);
        self.weight = self.weight - old_weight + self.scale.weight(&entry.key, &entry.value);
        result
    }

    pub fn retain(&mut self, mut filter: impl FnMut((&K, &V)) -> bool) {
        self.entries.retain(|index, entry| {
            let result = filter((&entry.key, &entry.value));
            if !result {
                let hash = self.hash_builder.hash_one(&entry.key);
                self.table
                    .find_entry(hash, |candidate| *candidate == index)
                    .expect("cache table missing live entry")
                    .remove();
                self.weight -= self.scale.weight(&entry.key, &entry.value);
            }
            result
        })
    }

    // if it cannot fit, returns value as error
    pub fn get_or_insert(&mut self, key: K, f: impl FnOnce() -> V) -> Result<(&V, usize), V> {
        let hash = self.hash_builder.hash_one(&key);
        if let Some(index) = self
            .table
            .find(hash, |index| self.entries.get(*index).key == key)
            .copied()
        {
            self.entries.promote(index);
            return Ok((&self.entries.get(index).value, index));
        }

        let value = f();
        let weight = self.scale.weight(&key, &value);
        if TRIM_ON_INSERT && weight > self.max_weight {
            return Err(value);
        }
        let (_, index) = self.entries.insert(Entry { key, value });
        let entries = &self.entries;
        let hash_builder = &self.hash_builder;
        self.table.insert_unique(hash, index, |index| {
            hash_builder.hash_one(&entries.get(*index).key)
        });
        self.weight += weight;
        if TRIM_ON_INSERT {
            self.trim_to_weight();
        }
        Ok((&self.entries.get(index).value, index))
    }

    pub fn trim_to_weight(&mut self) {
        while self.weight > self.max_weight {
            let (entry, index) = self.entries.pop_with_index().unwrap();
            let hash = self.hash_builder.hash_one(&entry.key);
            self.table
                .find_entry(hash, |candidate| *candidate == index)
                .expect("cache table missing live entry")
                .remove();
            self.weight -= self.scale.weight(&entry.key, &entry.value);
        }
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

        assert_eq!(
            cache
                .get_or_insert(1, || 10)
                .map(|(value, index)| (*value, index)),
            Ok((10, 0))
        );
        cache.get_or_insert(2, || 20).unwrap();
        cache.get_or_insert(3, || 30).unwrap();

        assert_eq!(cache.table.len(), 3);
        assert_eq!(cache.weight, 3);

        cache.trim_to_weight();

        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), Some(&20));
        assert_eq!(cache.get(&3), Some(&30));
        assert_eq!(cache.weight, 2);
    }

    #[test]
    fn immediate_eviction() {
        let mut cache = Cache::new(UnitWeight, 2);

        cache.get_or_insert(1, || 10).unwrap();
        cache.get_or_insert(2, || 20).unwrap();
        assert_eq!(cache.get(&1), Some(&10));
        cache.get_or_insert(3, || 30).unwrap();

        assert_eq!(cache.get(&1), Some(&10));
        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&3), Some(&30));
        assert_eq!(cache.table.len(), 2);
    }

    #[test]
    fn index_access_updates_weight() {
        let mut cache = DeferredCache::new(ValueWeight, 2);

        let (_, first) = cache.get_or_insert(1, || 1).unwrap();
        cache.get_or_insert(2, || 1).unwrap();
        cache.update_index(first, |value| *value = 2);

        assert_eq!(cache.get_index(first), &2);
        assert_eq!(cache.weight, 3);

        cache.trim_to_weight();

        assert_eq!(cache.get(&1), Some(&2));
        assert_eq!(cache.get(&2), None);
    }
}
