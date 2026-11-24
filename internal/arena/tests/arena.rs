use std::panic::{AssertUnwindSafe, catch_unwind};

use blit_arena::{Arena, Index};

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
fn values_are_dropped_once() {
    use core::cell::Cell;

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
