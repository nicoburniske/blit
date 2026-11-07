use std::{
    cell::Cell,
    hash::{DefaultHasher, Hash, Hasher},
    panic::AssertUnwindSafe,
    rc::Rc,
};

use blit::PhysicalRect;
use blit_gui::display_list::{
    ClipId, Command, CustomCommand, DisplayList,
    arena::{Arena, Handle, Slice},
};

struct Data {
    points: Slice<[i32; 2]>,
    resource: Handle<Rc<str>>,
}

impl CustomCommand for Data {
    type View<'a> = (&'a [[i32; 2]], &'a str);

    fn view<'a>(&'a self, arena: &'a Arena) -> Self::View<'a> {
        (arena.slice(self.points), arena.get(self.resource).as_ref())
    }
}

#[test]
fn bump_payloads_compare_by_content_and_release_owned_resources() {
    let resource: Rc<str> = Rc::from("retained across frames");
    let mut left = DisplayList::default();
    let mut right = DisplayList::default();
    for list in [&mut left, &mut right] {
        list.push_custom(PhysicalRect::new(0, 0, 10, 10), ClipId::default(), |data| Data {
            resource: data.push(resource.clone()),
            points: data.extend((0..10_000).map(|index| [index, index + 1])),
        });
        for index in 0..1000 {
            list.push_custom(PhysicalRect::default(), ClipId::default(), |_| index);
        }
    }
    assert_eq!(Rc::strong_count(&resource), 3);
    assert!(left.equivalent(0, &right, 0));
    let mut hashes = [DefaultHasher::new(), DefaultHasher::new()];
    for (list, state) in [&left, &right].into_iter().zip(&mut hashes) {
        let Command::Custom(custom) = list.get(0).command else {
            panic!()
        };
        custom.hash(state);
        let (points, text) = custom.get::<Data>().unwrap();
        assert_eq!(points[9999], [9999, 10000]);
        assert_eq!(text, resource.as_ref());
        assert!(custom.get::<u64>().is_none());
    }
    assert_eq!(hashes[0].finish(), hashes[1].finish());
    std::mem::swap(&mut left, &mut right);
    left.clear();
    assert_eq!(Rc::strong_count(&resource), 2);
    drop(right);
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
fn panicking_command_builders_still_drop_initialized_values() {
    struct Resource(Rc<Cell<usize>>);
    impl Drop for Resource {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let dropped = Rc::new(Cell::new(0));
    let mut list = DisplayList::default();
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        list.push_custom::<u32>(PhysicalRect::default(), ClipId::default(), |data| {
            data.push(Resource(dropped.clone()));
            panic!("failed to build command")
        });
    }));
    assert!(result.is_err());
    list.clear();
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
