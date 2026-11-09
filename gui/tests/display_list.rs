use std::{
    hash::Hash,
    sync::atomic::{AtomicUsize, Ordering},
};

use blit::{PhysicalRect, internal::hash::FxHasher};
use blit_gui::display_list::{DisplayList, Operations};

#[test]
fn typed_operations_preserve_callback_state_after_erasure() {
    static COMPARISONS: AtomicUsize = AtomicUsize::new(0);
    static HASHES: AtomicUsize = AtomicUsize::new(0);
    let operations = const {
        let captured = 42u64;
        &Operations::new(
            move |a: &String, _, b, _| {
                assert_eq!(captured, 42);
                COMPARISONS.fetch_add(1, Ordering::Relaxed);
                a == b
            },
            move |value, _, state| {
                assert_eq!(captured, 42);
                HASHES.fetch_add(1, Ordering::Relaxed);
                value.hash(state);
            },
            [42u64; 3],
        )
    };
    let bounds = PhysicalRect::new(0, 0, 8, 8);
    let mut left = DisplayList::default();
    let mut right = DisplayList::default();
    left.push(bounds, 0, String::from("same input"), operations);
    right.push(bounds, 0, String::from("same input"), operations);
    assert_eq!(COMPARISONS.load(Ordering::Relaxed), 0);
    assert_eq!(HASHES.load(Ordering::Relaxed), 0);
    assert_eq!(left.commands()[0].backend(), &[42; 3]);
    assert!(left.equivalent(0, &right, 0));
    assert_eq!(COMPARISONS.load(Ordering::Relaxed), 1);
    let mut a = FxHasher::default();
    let mut b = FxHasher::default();
    left.hash(0, &mut a);
    right.hash(0, &mut b);
    assert_eq!(HASHES.load(Ordering::Relaxed), 2);
    assert_eq!(a.finish(), b.finish());
    left.clear();
    left.push(bounds, 0, String::from("changed input"), operations);
    assert!(!left.equivalent(0, &right, 0));
}
