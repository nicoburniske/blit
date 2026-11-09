use std::{cell::Cell, panic::AssertUnwindSafe, rc::Rc};

use blit_cpu::arena::{Arena, Handle};

#[test]
fn values_survive_growth_and_release_resources() {
    assert_eq!(size_of::<Handle<Rc<str>>>(), 8);
    let resource: Rc<str> = Rc::from("retained for the frame");
    let mut arena = Arena::default();
    let value = arena.push(resource.clone());
    let points = arena.extend((0..10_000).map(|index| [index, index + 1]));
    for index in 0..1000 {
        arena.push(index);
    }
    assert_eq!(arena.get(value).as_ref(), resource.as_ref());
    assert_eq!(arena.slice(points)[9999], [9999, 10000]);
    assert_eq!(Rc::strong_count(&resource), 2);
    let erased = value.erase();
    assert!(arena.value::<u64>(erased).is_none());
    assert_eq!(arena.value::<Rc<str>>(erased).unwrap().as_ref(), resource.as_ref());
    arena.clear();
    assert_eq!(Rc::strong_count(&resource), 1);
}

#[test]
fn foreign_and_expired_handles_are_rejected() {
    let mut arena = Arena::default();
    let mut other = Arena::default();
    let value = arena.push(42u32);
    other.push(99u32);
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| other.get(value))).is_err());
    arena.clear();
    arena.push(0u32);
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| arena.get(value))).is_err());
    let slice = arena.extend([1u8, 2, 3].into_iter());
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| other.slice(slice))).is_err());
}

#[test]
fn interrupted_preparation_still_releases_initialized_values() {
    struct Resource(Rc<Cell<usize>>);
    impl Drop for Resource {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let dropped = Rc::new(Cell::new(0));
    let mut arena = Arena::default();
    assert!(
        std::panic::catch_unwind(AssertUnwindSafe(|| {
            arena.writer().push(Resource(dropped.clone()));
            panic!("failed to prepare drawing data")
        }))
        .is_err()
    );
    arena.clear();
    assert_eq!(dropped.get(), 1);
}

#[test]
fn panicking_destructors_invalidate_handles_before_dropping_values() {
    struct Resource(String);
    impl Drop for Resource {
        fn drop(&mut self) {
            panic!("{}", self.0);
        }
    }
    let mut arena = Arena::default();
    let value = arena.push(Resource("dropped".into()));
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| arena.clear())).is_err());
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| arena.get(value))).is_err());
    arena.clear();
}
