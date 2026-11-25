use std::{panic::AssertUnwindSafe, rc::Rc};

use blit::{LogicalRect, PhysicalRect};
use blit_gui::{
    damage::{DamageTracker, command::CommandDiff, tile::TileDamage},
    display_list::DisplayList,
};

#[test]
fn damage_tracking_and_backend_dispatch_share_a_display_list() {
    type Replay = fn(&DisplayList, usize, &mut usize);

    fn draw(list: &DisplayList, index: usize, output: &mut usize) {
        let (value, _) = list.input::<String>(index);
        *output = value.len();
    }

    fn check<D: DamageTracker>(mut damage: D) {
        let bounds = PhysicalRect::new(0, 0, 8, 8);
        let mut list = DisplayList::default();
        for (value, changed) in [("same", true), ("same", false), ("changed", true)] {
            list.push(
                LogicalRect::new(0.0, 0.0, 8.0, 8.0),
                bounds,
                0,
                String::from(value),
                draw as Replay,
            );
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
            list.replay::<Replay>(0)(&list, 0, &mut output);
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
    list.push(LogicalRect::default(), PhysicalRect::default(), 0, resource.clone(), ());
    let mut points = list.store_mut().slice(10_000);
    points.extend((0..10_000).map(|index| [index, index + 1]));
    let points = points.finish();
    let (value, _) = list.input::<Rc<str>>(0);
    assert_eq!(value.as_ref(), resource.as_ref());
    assert_eq!(list.store().view(points)[9999], [9999, 10000]);
    assert_eq!(Rc::strong_count(&resource), 2);
    list.clear();
    assert_eq!(Rc::strong_count(&resource), 1);
}

#[test]
fn input_and_replay_reject_wrong_types() {
    let mut list = DisplayList::default();
    list.push(
        LogicalRect::default(),
        PhysicalRect::default(),
        0,
        1_u32,
        (|_: &mut usize| {}) as fn(&mut usize),
    );
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| list.input::<u64>(0))).is_err());
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| list.replay::<fn(&mut u32)>(0))).is_err());
}

#[test]
fn slice_builders_publish_only_initialized_values_without_copying() {
    let mut list = DisplayList::default();
    let mut values = list.store_mut().slice(8);
    values.extend([1u32, 2, 3]);
    let pointer = values.as_ptr();
    let values = values.finish();
    assert_eq!(list.store().view(values), &[1, 2, 3]);
    assert_eq!(list.store().view(values).as_ptr(), pointer);
    list.store_mut().slice::<u64>(10_000);
    assert_eq!(list.store().view(values), &[1, 2, 3]);
}

#[test]
fn foreign_and_expired_slices_are_rejected() {
    let mut list = DisplayList::default();
    let mut other = DisplayList::default();
    let mut slice = list.store_mut().slice(3);
    slice.extend([1u8, 2, 3]);
    let slice = slice.finish();
    other.store_mut().slice::<u64>(4);
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| other.store().view(slice))).is_err());
    list.clear();
    list.store_mut().slice::<u64>(4);
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| list.store().view(slice))).is_err());
}

#[test]
fn panicking_destructors_invalidate_slices_before_dropping_values() {
    #[derive(PartialEq, Hash)]
    struct Resource;
    impl Drop for Resource {
        fn drop(&mut self) {
            panic!("interrupted cleanup");
        }
    }
    let mut list = DisplayList::default();
    let mut slice = list.store_mut().slice(1);
    slice.push(1u8);
    let slice = slice.finish();
    list.push(LogicalRect::default(), PhysicalRect::default(), 0, Resource, ());
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| list.clear())).is_err());
    assert!(std::panic::catch_unwind(AssertUnwindSafe(|| list.store().view(slice))).is_err());
    list.clear();
}
