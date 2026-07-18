#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub struct LruList<T> {
    capacity: usize,
    items: Vec<Node<T>>,
    least_recent: usize,
    most_recent: usize,
    free_first: usize,
}

// if empty, next is the next free index
// if present, prev/next are least/most recently used
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
struct Node<T> {
    value: Option<T>,
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
        self.items[index].value.as_ref().unwrap()
    }

    // err if no more space
    pub fn insert(&mut self, value: T) -> Result<(&T, usize), T> {
        let index = if self.free_first == usize::MAX {
            // no free list. need to insert
            if self.items.len() == self.capacity {
                return Err(value);
            }

            self.items.push(Node {
                value: None,
                prev: usize::MAX,
                next: usize::MAX,
            });
            self.items.len() - 1
        } else {
            // use the free list
            let index = self.free_first;
            self.free_first = self.items[index].next;
            index
        };

        if let Some(node) = self.items.get_mut(self.most_recent) {
            node.next = index;
        } else {
            self.least_recent = index;
        }

        self.items[index] = Node {
            value: Some(value),
            prev: self.most_recent,
            next: usize::MAX,
        };

        self.most_recent = index;

        Ok((self.items[index].value.as_ref().unwrap(), index))
    }

    // marks the entry as most recently used
    pub fn promote(&mut self, index: usize) {
        if index == self.most_recent {
            return;
        }

        let (node_next, node_prev) = {
            let node = &mut self.items[index];
            assert!(node.value.is_some(), "promoting used value");
            let next = node.next;
            let prev = node.prev;
            node.next = usize::MAX;
            node.prev = self.most_recent;
            (next, prev)
        };

        if let Some(node) = self.items.get_mut(node_prev) {
            node.next = node_next;
        }

        if let Some(node) = self.items.get_mut(node_next) {
            node.prev = node_prev;
        }

        if let Some(node) = self.items.get_mut(self.most_recent) {
            node.next = index;
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
        let (old_prev, old_next, value) = {
            let node = self.items.get_mut(index)?;

            let old_prev = node.prev;
            let old_next = node.next;

            node.next = self.free_first;
            node.prev = usize::MAX;

            (old_prev, old_next, node.value.take().unwrap())
        };

        self.free_first = index;

        if self.least_recent == index {
            self.least_recent = old_next;
        }

        if self.most_recent == index {
            self.most_recent = old_prev;
        }

        if let Some(node) = self.items.get_mut(old_next) {
            node.prev = old_prev;
        }
        if let Some(node) = self.items.get_mut(old_prev) {
            node.next = old_next;
        }

        Some(value)
    }

    pub fn retain(&mut self, mut filter: impl FnMut(usize, &T) -> bool) {
        let mut index = self.least_recent;
        while index != usize::MAX {
            let node = &self.items[index];
            let value = node.value.as_ref().unwrap();
            let next = node.next;
            if !filter(index, value) {
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
        (
            $lru:ident,
            recent: [least: $least_recent:expr, most: $most_recent:expr],
            free: $free_first:expr,
            nodes: [$(($value:expr, prev: $prev:expr, next: $next:expr)),* $(,)?],
        ) => {{
            let expected = LruList {
                capacity: $lru.capacity,
                items: vec![$(Node {
                    value: $value,
                    prev: $prev,
                    next: $next,
                }),*],
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
            nodes: [(Some(0), prev: usize::MAX, next: usize::MAX)],
        }

        lru.insert(1).unwrap();
        assert_lru! {
            lru,
            recent: [least: 0, most: 1],
            free: usize::MAX,
            nodes: [
                (Some(0), prev: usize::MAX, next: 1),
                (Some(1), prev: 0, next: usize::MAX),
            ],
        }

        lru.insert(2).unwrap();
        assert_lru! {
            lru,
            recent: [least: 0, most: 2],
            free: usize::MAX,
            nodes: [
                (Some(0), prev: usize::MAX, next: 1),
                (Some(1), prev: 0, next: 2),
                (Some(2), prev: 1, next: usize::MAX),
            ],
        }

        lru.promote(0);
        assert_lru! {
            lru,
            recent: [least: 1, most: 0],
            free: usize::MAX,
            nodes: [
                (Some(0), prev: 2, next: usize::MAX),
                (Some(1), prev: usize::MAX, next: 2),
                (Some(2), prev: 1, next: 0),
            ],
        }

        assert_eq!(lru.pop(), Some(1));
        assert_lru! {
            lru,
            recent: [least: 2, most: 0],
            free: 1,
            nodes: [
                (Some(0), prev: 2, next: usize::MAX),
                (None, prev: usize::MAX, next: usize::MAX),
                (Some(2), prev: usize::MAX, next: 0),
            ],
        }

        assert_eq!(lru.pop(), Some(2));
        assert_lru! {
            lru,
            recent: [least: 0, most: 0],
            free: 2,
            nodes: [
                (Some(0), prev: usize::MAX, next: usize::MAX),
                (None, prev: usize::MAX, next: usize::MAX),
                (None, prev: usize::MAX, next: 1),
            ],
        }

        assert_eq!(lru.pop(), Some(0));
        assert_lru! {
            lru,
            recent: [least: usize::MAX, most: usize::MAX],
            free: 0,
            nodes: [
                (None, prev: usize::MAX, next: 2),
                (None, prev: usize::MAX, next: usize::MAX),
                (None, prev: usize::MAX, next: 1),
            ],
        }

        lru.insert(0).unwrap();
        assert_lru! {
            lru,
            recent: [least: 0, most: 0],
            free: 2,
            nodes: [
                (Some(0), prev: usize::MAX, next: usize::MAX),
                (None, prev: usize::MAX, next: usize::MAX),
                (None, prev: usize::MAX, next: 1),
            ],
        }

        lru.promote(0);
        assert_lru! {
            lru,
            recent: [least: 0, most: 0],
            free: 2,
            nodes: [
                (Some(0), prev: usize::MAX, next: usize::MAX),
                (None, prev: usize::MAX, next: usize::MAX),
                (None, prev: usize::MAX, next: 1),
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
                (Some(2), prev: 1, next: usize::MAX),
                (Some(1), prev: usize::MAX, next: 0),
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
                (Some(0), prev: usize::MAX, next: 2),
                (Some(1), prev: 2, next: usize::MAX),
                (Some(2), prev: 0, next: 1),
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
                (Some(0), prev: usize::MAX, next: 2),
                (None, prev: usize::MAX, next: usize::MAX),
                (Some(2), prev: 0, next: usize::MAX),
            ],
        }

        assert_eq!(lru.remove(0), Some(0));
        assert_lru! {
            lru,
            recent: [least: 2, most: 2],
            free: 0,
            nodes: [
                (None, prev: usize::MAX, next: 1),
                (None, prev: usize::MAX, next: usize::MAX),
                (Some(2), prev: usize::MAX, next: usize::MAX),
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
                (None, prev: usize::MAX, next: usize::MAX),
                (Some(1), prev: usize::MAX, next: usize::MAX),
                (None, prev: usize::MAX, next: 0),
            ],
        }
    }
}
