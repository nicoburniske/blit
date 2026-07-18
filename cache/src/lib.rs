//! weighted lru cache

use std::hash::BuildHasher;

use hashbrown::{HashTable, hash_map::DefaultHashBuilder};

pub trait Scale<K, V> {
    fn weight(&self, key: &K, value: &V) -> usize;
}

pub struct Cache<K, V, S: Scale<K, V>> {
    hash_builder: DefaultHashBuilder,
    table: HashTable<usize>,
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
            hash_builder: DefaultHashBuilder::default(),
            table: HashTable::new(),
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
        use hashbrown::hash_table::Entry as TableEntry;
        let hash = self.hash_builder.hash_one(&key);
        match self.table.entry(
            hash,
            |index| self.entries.get(*index).key == key,
            |index| self.hash_builder.hash_one(&self.entries.get(*index).key),
        ) {
            TableEntry::Occupied(entry) => {
                let index = *entry.get();
                self.entries.promote(index);
                Ok(&self.entries.get(index).value)
            }
            TableEntry::Vacant(vacant) => {
                let value = f();
                let weight = self.scale.weight(&key, &value);
                if weight > self.max_weight {
                    return Err(value);
                }
                while self.weight + weight > self.max_weight {
                    let entry = self.entries.pop().unwrap();
                    self.weight -= self.scale.weight(&entry.key, &entry.value);
                }
                let (entry, index) = self
                    .entries
                    .insert(Entry { key, value })
                    .map_err(|e| e.value)?;
                vacant.insert(index);
                self.weight += weight;
                Ok(&entry.value)
            }
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
}

mod list;

#[cfg(test)]
mod test {}
