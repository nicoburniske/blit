use std::{any::TypeId, hash::Hash, ptr::NonNull};

use blit::PhysicalRect;
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
                    unsafe { left.load::<String>(a) == right.load::<String>(b) }
                },
                hash: |data, offset, state| {
                    // safety: insertion pairs this table with String
                    unsafe { data.load::<String>(offset) }.hash(state);
                },
                dispatch: |command, data, context| {
                    // safety: the test pairs String inputs with a usize output
                    let value = unsafe { data.load::<String>(command.offset()) };
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
            unsafe { list.commands()[0].dispatch(list.data(), NonNull::from(&mut output).cast()) };
            assert_eq!(output, value.len());
            damage.finish_frame(&mut list);
            list.clear();
        }
    }

    check(TileDamage::new(8));
    check(CommandDiff::default());
}
