use std::{any::TypeId, hash::Hash, panic::AssertUnwindSafe, ptr::NonNull, rc::Rc};

use blit::{PhysicalRect, internal::bump::Offset};
use blit_gui::{
    damage::{DamageTracker, command::CommandDiff, tile::TileDamage},
    display_list::{DisplayList, Ops},
};

#[test]
fn damage_tracking_and_backend_dispatch_share_a_display_list() {
    fn check<D: DamageTracker>(mut damage: D) {
        let ops = const {
            &Ops {
                kind: TypeId::of::<String>(),
                equal: |left, a, right, b| {
                    // safety: insertion pairs this table with String
                    unsafe { left.data().get(Offset::<String>::new(a)) == right.data().get(Offset::<String>::new(b)) }
                },
                hash: |list, offset, state| {
                    // safety: insertion pairs this table with String
                    unsafe { list.data().get(Offset::<String>::new(offset)) }.hash(state);
                },
                dispatch: |command, list, context| {
                    // safety: the test pairs String inputs with a usize output
                    let value = unsafe { list.data().get(Offset::<String>::new(command.offset())) };
                    unsafe { *context.cast::<usize>().as_mut() = value.len() };
                },
            }
        };
        let bounds = PhysicalRect::new(0, 0, 8, 8);
        let mut list = DisplayList::default();
        for (value, changed) in [("same", true), ("same", false), ("changed", true)] {
            // safety: the table describes the owned String and its comparison semantics
            unsafe { list.push(bounds, 0, String::from(value), ops) };
            damage.update(&list, bounds, 1.0, 1);
            assert_eq!(damage.is_empty(), !changed);
            assert_eq!(damage.intersects(bounds, 0), changed);
            let mut ranges = Vec::new();
            assert_eq!(damage.line_ranges(0, bounds, &mut ranges), 8);
            assert_eq!(ranges.len(), usize::from(changed));
            if changed {
                assert_eq!(ranges[0], 0..8);
            }
            let mut output = 0usize;
            // safety: this command belongs to list and its dispatch expects a usize output
            unsafe { list.commands()[0].dispatch(&list, NonNull::from(&mut output).cast()) };
            assert_eq!(output, value.len());
            damage.finish_frame(&mut list);
            list.clear();
        }
    }

    check(TileDamage::new(8));
    check(CommandDiff::default());
}

#[test]
fn values_and_slices_survive_growth_and_release_resources() {
    let resource: Rc<str> = Rc::from("retained for the frame");
    let mut list = DisplayList::default();
    // safety: the table describes Rc<str> and does not access backend state
    unsafe { list.push(PhysicalRect::default(), 0, resource.clone(), ops::<Rc<str>>()) };
    let points = list.store_slice((0..10_000).map(|index| [index, index + 1]));
    // safety: the first command stores Rc<str>
    let value = unsafe { list.data().get(Offset::<Rc<str>>::new(list.commands()[0].offset())) };
    assert_eq!(value.as_ref(), resource.as_ref());
    assert_eq!(list.slice(points)[9999], [9999, 10000]);
    assert_eq!(Rc::strong_count(&resource), 2);
    list.clear();
    assert_eq!(Rc::strong_count(&resource), 1);
}

#[test]
fn foreign_and_expired_slices_are_rejected() {
    let mut list = DisplayList::default();
    let mut other = DisplayList::default();
    let slice = list.store_slice([1u8, 2, 3].into_iter());
    other.store_slice([0u64; 4].into_iter());
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| other.slice(slice))).is_err());
    list.clear();
    list.store_slice([0u64; 4].into_iter());
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| list.slice(slice))).is_err());
}

#[test]
fn panicking_destructors_invalidate_slices_before_dropping_values() {
    struct Resource;
    impl Drop for Resource {
        fn drop(&mut self) {
            panic!("interrupted cleanup");
        }
    }
    let mut list = DisplayList::default();
    let slice = list.store_slice([1u8].into_iter());
    // safety: the table describes Resource and does not access backend state
    unsafe { list.push(PhysicalRect::default(), 0, Resource, ops::<Resource>()) };
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| list.clear())).is_err());
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| list.slice(slice))).is_err());
    list.clear();
}

fn ops<T: 'static>() -> &'static Ops {
    const {
        &Ops {
            kind: TypeId::of::<T>(),
            equal: |_, _, _, _| true,
            hash: |_, _, _| {},
            dispatch: |_, _, _| {},
        }
    }
}
