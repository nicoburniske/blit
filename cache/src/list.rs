use std::hint::unreachable_unchecked;

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub struct LruList<T> {
    capacity: usize,
    items: Vec<Node<T>>,
    least_recent: usize,
    most_recent: usize,
    free_first: usize,
}

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
enum Node<T> {
    Value(Value<T>),
    Empty { next: usize },
}

macro_rules! value_unchecked {
    ($node:expr) => {
        match $node {
            Node::Value(value) => value,
            Node::Empty { .. } => {
                debug_assert!(false, "expected value node");
                // safety: callers only pass nodes in the active list
                unsafe { unreachable_unchecked() }
            }
        }
    };
}

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
struct Value<T> {
    value: T,
    prev: usize,
    next: usize,
}

impl<T> LruList<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            items: Vec::new(),
            least_recent: usize::MAX,
            most_recent: usize::MAX,
            free_first: usize::MAX,
        }
    }

    pub fn get(&self, index: usize) -> &T {
        let Node::Value(value) = &self.items[index] else {
            panic!("no value")
        };
        &value.value
    }

    // err if no more space
    pub fn insert(&mut self, value: T) -> Result<(&T, usize), T> {
        let index = if self.free_first == usize::MAX {
            // no free list. need to insert
            if self.items.len() == self.capacity {
                return Err(value);
            }

            self.items.push(Node::Value(Value {
                value,
                prev: self.most_recent,
                next: usize::MAX,
            }));
            self.items.len() - 1
        } else {
            // use the free list
            let index = self.free_first;
            let Node::Empty { next } = self.items[index] else {
                // safety: free list only has free nodes
                unsafe { unreachable_unchecked() }
            };
            self.free_first = next;
            self.items[index] = Node::Value(Value {
                value,
                prev: self.most_recent,
                next: usize::MAX,
            });
            index
        };

        if let Some(value) = self.items.get_mut(self.most_recent) {
            value_unchecked!(value).next = index;
        } else {
            self.least_recent = index;
        }

        self.most_recent = index;

        let value = value_unchecked!(&self.items[index]);

        Ok((&value.value, index))
    }

    // marks the entry as most recently used
    pub fn promote(&mut self, index: usize) {
        if index == self.most_recent {
            return;
        }

        let (node_next, node_prev) = {
            let Node::Value(node) = &mut self.items[index] else {
                panic!("no value")
            };
            let next = node.next;
            let prev = node.prev;
            node.next = usize::MAX;
            node.prev = self.most_recent;
            (next, prev)
        };

        if let Some(node) = self.items.get_mut(node_prev) {
            value_unchecked!(node).next = node_next;
        }

        if let Some(node) = self.items.get_mut(node_next) {
            value_unchecked!(node).prev = node_prev;
        }

        if let Some(node) = self.items.get_mut(self.most_recent) {
            value_unchecked!(node).next = index;
        }

        self.most_recent = index;

        if self.least_recent == index {
            self.least_recent = node_next;
        }
    }

    // removes least recently used
    pub fn pop(&mut self) -> Option<T> {
        let index = self.least_recent;
        self.remove(index)
    }

    pub fn remove(&mut self, index: usize) -> Option<T> {
        let removed = {
            let item = self.items.get_mut(index)?;
            let Node::Value(value) = std::mem::replace(
                item,
                Node::Empty {
                    next: self.free_first,
                },
            ) else {
                panic!("not a value")
            };
            value
        };

        self.free_first = index;

        if self.least_recent == index {
            self.least_recent = removed.next;
        }

        if self.most_recent == index {
            self.most_recent = removed.prev;
        }

        if let Some(node) = self.items.get_mut(removed.next) {
            value_unchecked!(node).prev = removed.prev;
        }
        if let Some(node) = self.items.get_mut(removed.prev) {
            value_unchecked!(node).next = removed.next;
        }

        Some(removed.value)
    }

    pub fn retain(&mut self, mut filter: impl FnMut(usize, &T) -> bool) {
        let mut index = self.least_recent;
        while index != usize::MAX {
            let node = &self.items[index];
            let value = value_unchecked!(node);
            let next = value.next;
            if !filter(index, &value.value) {
                self.remove(index);
            }
            index = next;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    macro_rules! assert_lru {
        (@node Value { value: $value:expr, prev: $prev:expr, next: $next:expr }) => {
            Node::Value(Value {
                value: $value,
                prev: $prev,
                next: $next,
            })
        };
        (@node Empty { next: $next:expr }) => {
            Node::Empty { next: $next }
        };
        (
            $lru:ident,
            recent: [least: $least_recent:expr, most: $most_recent:expr],
            free: $free_first:expr,
            nodes: [$($kind:ident { $($fields:tt)* }),* $(,)?],
        ) => {{
            let expected = LruList {
                capacity: $lru.capacity,
                items: vec![$(assert_lru!(@node $kind { $($fields)* })),*],
                least_recent: $least_recent,
                most_recent: $most_recent,
                free_first: $free_first,
            };
            assert!(
                $lru == expected,
                "lru mismatch\n\nactual:\n{:#?}\n\nexpected:\n{expected:#?}",
                $lru
            );
        }};
    }

    #[test]
    fn basic() {
        let mut lru = LruList::<u32>::new(5);

        lru.insert(0).unwrap();
        assert_lru! {
            lru,
            recent: [least: 0, most: 0],
            free: usize::MAX,
            nodes: [Value { value: 0, prev: usize::MAX, next: usize::MAX }],
        }

        lru.insert(1).unwrap();
        assert_lru! {
            lru,
            recent: [least: 0, most: 1],
            free: usize::MAX,
            nodes: [
                Value { value: 0, prev: usize::MAX, next: 1 },
                Value { value: 1, prev: 0, next: usize::MAX },
            ],
        }

        lru.insert(2).unwrap();
        assert_lru! {
            lru,
            recent: [least: 0, most: 2],
            free: usize::MAX,
            nodes: [
                Value { value: 0, prev: usize::MAX, next: 1 },
                Value { value: 1, prev: 0, next: 2 },
                Value { value: 2, prev: 1, next: usize::MAX },
            ],
        }

        lru.promote(0);
        assert_lru! {
            lru,
            recent: [least: 1, most: 0],
            free: usize::MAX,
            nodes: [
                Value { value: 0, prev: 2, next: usize::MAX },
                Value { value: 1, prev: usize::MAX, next: 2 },
                Value { value: 2, prev: 1, next: 0 },
            ],
        }

        assert_eq!(lru.pop(), Some(1));
        assert_lru! {
            lru,
            recent: [least: 2, most: 0],
            free: 1,
            nodes: [
                Value { value: 0, prev: 2, next: usize::MAX },
                Empty { next: usize::MAX },
                Value { value: 2, prev: usize::MAX, next: 0 },
            ],
        }

        assert_eq!(lru.pop(), Some(2));
        assert_lru! {
            lru,
            recent: [least: 0, most: 0],
            free: 2,
            nodes: [
                Value { value: 0, prev: usize::MAX, next: usize::MAX },
                Empty { next: usize::MAX },
                Empty { next: 1 },
            ],
        }

        assert_eq!(lru.pop(), Some(0));
        assert_lru! {
            lru,
            recent: [least: usize::MAX, most: usize::MAX],
            free: 0,
            nodes: [
                Empty { next: 2 },
                Empty { next: usize::MAX },
                Empty { next: 1 },
            ],
        }

        lru.insert(0).unwrap();
        assert_lru! {
            lru,
            recent: [least: 0, most: 0],
            free: 2,
            nodes: [
                Value { value: 0, prev: usize::MAX, next: usize::MAX },
                Empty { next: usize::MAX },
                Empty { next: 1 },
            ],
        }

        lru.promote(0);
        assert_lru! {
            lru,
            recent: [least: 0, most: 0],
            free: 2,
            nodes: [
                Value { value: 0, prev: usize::MAX, next: usize::MAX },
                Empty { next: usize::MAX },
                Empty { next: 1 },
            ],
        }
    }

    #[test]
    fn reuse_only_free_slot() {
        let mut lru = LruList::<u32>::new(2);
        lru.insert(0).unwrap();
        lru.insert(1).unwrap();
        assert_eq!(lru.remove(0), Some(0));
        lru.insert(2).unwrap();

        assert_lru! {
            lru,
            recent: [least: 1, most: 0],
            free: usize::MAX,
            nodes: [
                Value { value: 2, prev: 1, next: usize::MAX },
                Value { value: 1, prev: usize::MAX, next: 0 },
            ],
        }
    }

    #[test]
    fn promote_middle() {
        let mut lru = LruList::<u32>::new(3);
        lru.insert(0).unwrap();
        lru.insert(1).unwrap();
        lru.insert(2).unwrap();
        lru.promote(1);

        assert_lru! {
            lru,
            recent: [least: 0, most: 1],
            free: usize::MAX,
            nodes: [
                Value { value: 0, prev: usize::MAX, next: 2 },
                Value { value: 1, prev: 2, next: usize::MAX },
                Value { value: 2, prev: 0, next: 1 },
            ],
        }
    }

    #[test]
    fn remove() {
        let mut lru = LruList::<u32>::new(5);
        lru.insert(0).unwrap();
        lru.insert(1).unwrap();
        lru.insert(2).unwrap();

        assert_eq!(lru.remove(1), Some(1));
        assert_lru! {
            lru,
            recent: [least: 0, most: 2],
            free: 1,
            nodes: [
                Value { value: 0, prev: usize::MAX, next: 2 },
                Empty { next: usize::MAX },
                Value { value: 2, prev: 0, next: usize::MAX },
            ],
        }

        assert_eq!(lru.remove(0), Some(0));
        assert_lru! {
            lru,
            recent: [least: 2, most: 2],
            free: 0,
            nodes: [
                Empty { next: 1 },
                Empty { next: usize::MAX },
                Value { value: 2, prev: usize::MAX, next: usize::MAX },
            ],
        }
    }

    #[test]
    fn retain() {
        let mut lru = LruList::<u32>::new(5);
        lru.insert(0).unwrap();
        lru.insert(1).unwrap();
        lru.insert(2).unwrap();

        lru.retain(|index, _| index % 2 != 0);

        assert_lru! {
            lru,
            recent: [least: 1, most: 1],
            free: 2,
            nodes: [
                Empty { next: usize::MAX },
                Value { value: 1, prev: usize::MAX, next: usize::MAX },
                Empty { next: 0 },
            ],
        }
    }
}
