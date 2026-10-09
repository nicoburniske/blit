use blit::PhysicalRect;
use blit_arrayvec::ArrayVec;
use blit_gui::display_list::DisplayList;

pub const MAX_DAMAGE: usize = 32;

pub fn diff(
    previous: &DisplayList,
    display_list: &DisplayList,
    damage: &mut ArrayVec<PhysicalRect, { MAX_DAMAGE * 2 }>,
) {
    let mut start = 0;
    let common = previous.len().min(display_list.len());
    while start < common && previous.equivalent(start, display_list, start) {
        start += 1;
    }
    let mut old_end = previous.len();
    let mut new_end = display_list.len();
    while old_end > start && new_end > start && previous.equivalent(old_end - 1, display_list, new_end - 1) {
        old_end -= 1;
        new_end -= 1;
    }
    let mut old = start;
    let mut new = start;
    while old < old_end && new < new_end {
        if previous.equivalent(old, display_list, new) {
            old += 1;
            new += 1;
            continue;
        }
        let mut aligned = false;
        for offset in 1..=8 {
            if new + offset < new_end && previous.equivalent(old, display_list, new + offset) {
                for index in new..new + offset {
                    push_damage(damage, display_list.get(index).bounds);
                }
                new += offset;
                aligned = true;
                break;
            }
            if old + offset < old_end && previous.equivalent(old + offset, display_list, new) {
                for index in old..old + offset {
                    push_damage(damage, previous.get(index).bounds);
                }
                old += offset;
                aligned = true;
                break;
            }
        }
        if aligned {
            continue;
        }
        let old_bounds = previous.get(old).bounds;
        let new_bounds = display_list.get(new).bounds;
        push_damage(damage, old_bounds);
        if new_bounds != old_bounds {
            push_damage(damage, new_bounds);
        }
        old += 1;
        new += 1;
    }
    for index in old..old_end {
        push_damage(damage, previous.get(index).bounds);
    }
    for index in new..new_end {
        push_damage(damage, display_list.get(index).bounds);
    }
}

fn push_damage(damage: &mut ArrayVec<PhysicalRect, { MAX_DAMAGE * 2 }>, bounds: PhysicalRect) {
    if bounds.width <= 0 || bounds.height <= 0 {
        return;
    }
    if damage.len() < MAX_DAMAGE {
        damage.push(bounds);
        return;
    }
    let len = damage.len();
    for index in 0..len / 2 {
        damage[index] = damage[index * 2].union(damage[index * 2 + 1]);
    }
    if len % 2 == 1 {
        damage[len / 2] = damage[len - 1];
    }
    damage.truncate(len.div_ceil(2));
    damage.push(bounds);
}
