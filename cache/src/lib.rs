//! weighted lru cache

mod list;

use std::hash::BuildHasher;

pub trait Scale<K, V> {
    fn weight(&self, key: &K, value: &V) -> usize;
}

pub struct Cache<K, V, S: Scale<K, V>> {
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

impl<K, V, S> Cache<K, V, S>
where
    K: std::hash::Hash + PartialEq,
    S: Scale<K, V>,
{
    pub fn new(scale: S, max_weight: usize, capacity: usize) -> Self {
        Self {
            hash_builder: Default::default(),
            table: Default::default(),
            entries: list::LruList::<Entry<K, V>>::new(capacity),
            scale,
            weight: 0,
            max_weight,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        let hash = self.hash_builder.hash_one(&key);
        let index = self
            .table
            .find(hash, |index| &self.entries.get(*index).key == key)
            .copied()?;
        self.entries.promote(index);
        Some(&self.entries.get(index).value)
    }

    // if it cannot fit, returns value as error
    pub fn get_or_insert_with(&mut self, key: K, f: impl FnOnce() -> V) -> Result<&V, V> {
        self.get_or_insert(key, f, true).map(|(value, _)| value)
    }

    pub fn get_or_insert_deferred_with(
        &mut self,
        key: K,
        f: impl FnOnce() -> V,
    ) -> Result<(&V, usize), V> {
        self.get_or_insert(key, f, false)
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

    fn get_or_insert(
        &mut self,
        key: K,
        f: impl FnOnce() -> V,
        evict: bool,
    ) -> Result<(&V, usize), V> {
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
        if weight > self.max_weight {
            return Err(value);
        }
        if evict {
            while self.weight.saturating_add(weight) > self.max_weight {
                let (entry, index) = self.entries.pop_with_index().unwrap();
                let hash = self.hash_builder.hash_one(&entry.key);
                self.table
                    .find_entry(hash, |candidate| *candidate == index)
                    .expect("cache table missing live entry")
                    .remove();
                self.weight -= self.scale.weight(&entry.key, &entry.value);
            }
        }
        let index = match self.entries.insert(Entry { key, value }) {
            Ok((_, index)) => index,
            Err(entry) => return Err(entry.value),
        };
        let entries = &self.entries;
        let hash_builder = &self.hash_builder;
        self.table.insert_unique(hash, index, |index| {
            hash_builder.hash_one(&entries.get(*index).key)
        });
        self.weight += weight;
        Ok((&self.entries.get(index).value, index))
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

    #[test]
    fn deferred_eviction() {
        let mut cache = Cache::new(UnitWeight, 2, 3);

        assert_eq!(
            cache
                .get_or_insert_deferred_with(1, || 10)
                .map(|(value, index)| (*value, index)),
            Ok((10, 0))
        );
        cache.get_or_insert_deferred_with(2, || 20).unwrap();
        cache.get_or_insert_deferred_with(3, || 30).unwrap();

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
        let mut cache = Cache::new(UnitWeight, 2, 3);

        cache.get_or_insert_with(1, || 10).unwrap();
        cache.get_or_insert_with(2, || 20).unwrap();
        assert_eq!(cache.get(&1), Some(&10));
        cache.get_or_insert_with(3, || 30).unwrap();

        assert_eq!(cache.get(&1), Some(&10));
        assert_eq!(cache.get(&2), None);
        assert_eq!(cache.get(&3), Some(&30));
        assert_eq!(cache.table.len(), 2);
    }
}
