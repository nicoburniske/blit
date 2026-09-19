// adapted from etagere 0.3.0
// https://github.com/nical/etagere/tree/2a178998597191e45631ea5f35d712352e15b0ef
//
// The MIT License (MIT)
//
// Copyright (c) 2020 Nicolas Silva
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software is furnished to do so,
// subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
// FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
// COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
// IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
// CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

const SHELF_SPLIT_THRESHOLD: u16 = 8;
const ITEM_SPLIT_THRESHOLD: u16 = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Slot(u16);

impl Slot {
    const NONE: Self = Self(u16::MAX);

    fn index(self) -> usize {
        self.0 as usize
    }

    fn is_some(self) -> bool {
        self != Self::NONE
    }
}

#[derive(Clone, Copy)]
struct Shelf {
    y: u16,
    height: u16,
    previous: Slot,
    next: Slot,
    first_free: Slot,
    empty: bool,
}

#[derive(Clone, Copy)]
struct Item {
    x: u16,
    width: u16,
    previous: Slot,
    next: Slot,
    previous_free: Slot,
    next_free: Slot,
    shelf: Slot,
    generation: u16,
    allocated: bool,
}

#[derive(Clone, Copy)]
pub struct AllocId {
    item: Slot,
    generation: u16,
}

pub struct Allocation {
    pub id: AllocId,
    pub position: [u32; 2],
}

pub struct AtlasAllocator {
    width: u16,
    height: u16,
    shelves: Vec<Shelf>,
    items: Vec<Item>,
    free_shelves: Slot,
    free_items: Slot,
    allocations: usize,
}

impl AtlasAllocator {
    pub fn new(size: [u32; 2]) -> Self {
        assert!(size[0] > 0 && size[1] > 0, "atlas size must be positive");
        let width = u16::try_from(size[0]).expect("atlas is too wide");
        let height = u16::try_from(size[1]).expect("atlas is too tall");
        Self {
            width,
            height,
            shelves: vec![Shelf {
                y: 0,
                height,
                previous: Slot::NONE,
                next: Slot::NONE,
                first_free: Slot(0),
                empty: true,
            }],
            items: vec![Item {
                x: 0,
                width,
                previous: Slot::NONE,
                next: Slot::NONE,
                previous_free: Slot::NONE,
                next_free: Slot::NONE,
                shelf: Slot(0),
                generation: 1,
                allocated: false,
            }],
            free_shelves: Slot::NONE,
            free_items: Slot::NONE,
            allocations: 0,
        }
    }

    pub fn allocate(&mut self, size: [u32; 2]) -> Option<Allocation> {
        if size[0] == 0
            || size[1] == 0
            || size[0] > u32::from(self.width)
            || size[1] > u32::from(self.height)
        {
            return None;
        }

        let width = size[0] as u16;
        let requested_height = size[1] as u16;
        let alignment = match requested_height {
            0..=31 => 8,
            32..=127 => 16,
            128..=511 => 32,
            _ => 64,
        };
        let rounded_height =
            u32::from(requested_height).saturating_add(alignment - 1) / alignment * alignment;
        let height = if rounded_height <= u32::from(self.height) {
            rounded_height as u16
        } else {
            requested_height
        };
        let bad_fit_height = height.saturating_add(height / 2);
        let mut selected_height = u16::MAX;
        let mut selected_shelf = Slot::NONE;
        let mut selected_item = Slot::NONE;
        let mut shelf_slot = Slot(0);

        while shelf_slot.is_some() {
            let shelf = self.shelves[shelf_slot.index()];
            if shelf.height >= height && shelf.height < selected_height {
                let bad_fit = !shelf.empty && shelf.height > bad_fit_height;
                if !bad_fit || !selected_shelf.is_some() {
                    let mut item_slot = shelf.first_free;
                    while item_slot.is_some() && self.items[item_slot.index()].width < width {
                        item_slot = self.items[item_slot.index()].next_free;
                    }
                    if item_slot.is_some() {
                        selected_shelf = shelf_slot;
                        selected_item = item_slot;
                        if !bad_fit {
                            selected_height = shelf.height;
                        }
                        if shelf.height == height {
                            break;
                        }
                    }
                }
            }
            shelf_slot = shelf.next;
        }

        if !selected_shelf.is_some() {
            return None;
        }

        let mut shelf = self.shelves[selected_shelf.index()];
        self.shelves[selected_shelf.index()].empty = false;
        if shelf.empty && shelf.height > height.saturating_add(SHELF_SPLIT_THRESHOLD) {
            let new_shelf = Shelf {
                y: shelf.y + height,
                height: shelf.height - height,
                previous: selected_shelf,
                next: shelf.next,
                first_free: Slot::NONE,
                empty: true,
            };
            let new_shelf_slot = if self.free_shelves.is_some() {
                let slot = self.free_shelves;
                self.free_shelves = self.shelves[slot.index()].next;
                self.shelves[slot.index()] = new_shelf;
                slot
            } else {
                assert!(
                    self.shelves.len() < u16::MAX as usize,
                    "too many atlas shelves"
                );
                let slot = Slot(self.shelves.len() as u16);
                self.shelves.push(new_shelf);
                slot
            };
            let new_item_slot = self.add_item(Item {
                x: 0,
                width: self.width,
                previous: Slot::NONE,
                next: Slot::NONE,
                previous_free: Slot::NONE,
                next_free: Slot::NONE,
                shelf: new_shelf_slot,
                generation: 1,
                allocated: false,
            });
            self.shelves[new_shelf_slot.index()].first_free = new_item_slot;
            self.shelves[selected_shelf.index()].height = height;
            self.shelves[selected_shelf.index()].next = new_shelf_slot;
            if shelf.next.is_some() {
                self.shelves[shelf.next.index()].previous = new_shelf_slot;
            }
            shelf.height = height;
        }

        let item = self.items[selected_item.index()];
        if item.width - width > ITEM_SPLIT_THRESHOLD {
            let new_item_slot = self.add_item(Item {
                x: item.x + width,
                width: item.width - width,
                previous: selected_item,
                next: item.next,
                previous_free: item.previous_free,
                next_free: item.next_free,
                shelf: item.shelf,
                generation: 1,
                allocated: false,
            });
            self.items[selected_item.index()].width = width;
            self.items[selected_item.index()].next = new_item_slot;
            if item.next.is_some() {
                self.items[item.next.index()].previous = new_item_slot;
            }
            if self.shelves[selected_shelf.index()].first_free == selected_item {
                self.shelves[selected_shelf.index()].first_free = new_item_slot;
            }
            if item.previous_free.is_some() {
                self.items[item.previous_free.index()].next_free = new_item_slot;
            }
            if item.next_free.is_some() {
                self.items[item.next_free.index()].previous_free = new_item_slot;
            }
        } else {
            if self.shelves[selected_shelf.index()].first_free == selected_item {
                self.shelves[selected_shelf.index()].first_free = item.next_free;
            }
            if item.previous_free.is_some() {
                self.items[item.previous_free.index()].next_free = item.next_free;
            }
            if item.next_free.is_some() {
                self.items[item.next_free.index()].previous_free = item.previous_free;
            }
        }

        self.items[selected_item.index()].allocated = true;
        self.allocations += 1;
        Some(Allocation {
            id: AllocId {
                item: selected_item,
                generation: self.items[selected_item.index()].generation,
            },
            position: [u32::from(item.x), u32::from(shelf.y)],
        })
    }

    pub fn deallocate(&mut self, id: AllocId) {
        let item_slot = id.item;
        let item = self.items[item_slot.index()];
        assert!(
            item.allocated && item.generation == id.generation,
            "invalid allocation"
        );
        self.items[item_slot.index()].allocated = false;
        self.allocations -= 1;

        let shelf_slot = item.shelf;
        let mut previous = item.previous;
        let mut next = item.next;
        let mut width = item.width;
        if next.is_some() && !self.items[next.index()].allocated {
            let free = self.items[next.index()];
            if self.shelves[shelf_slot.index()].first_free == next {
                self.shelves[shelf_slot.index()].first_free = free.next_free;
            }
            if free.previous_free.is_some() {
                self.items[free.previous_free.index()].next_free = free.next_free;
            }
            if free.next_free.is_some() {
                self.items[free.next_free.index()].previous_free = free.previous_free;
            }
            self.items[item_slot.index()].next = free.next;
            self.items[item_slot.index()].width += free.width;
            width += free.width;
            if free.next.is_some() {
                self.items[free.next.index()].previous = item_slot;
            }
            self.remove_item(next);
            next = free.next;
        }

        if previous.is_some() && !self.items[previous.index()].allocated {
            self.items[previous.index()].next = next;
            self.items[previous.index()].width += width;
            if next.is_some() {
                self.items[next.index()].previous = previous;
            }
            self.remove_item(item_slot);
            previous = self.items[previous.index()].previous;
        } else {
            let first_free = self.shelves[shelf_slot.index()].first_free;
            if first_free.is_some() {
                self.items[first_free.index()].previous_free = item_slot;
            }
            self.items[item_slot.index()].previous_free = Slot::NONE;
            self.items[item_slot.index()].next_free = first_free;
            self.shelves[shelf_slot.index()].first_free = item_slot;
        }

        if !previous.is_some() && !next.is_some() {
            self.shelves[shelf_slot.index()].empty = true;
            let next_shelf = self.shelves[shelf_slot.index()].next;
            if next_shelf.is_some() && self.shelves[next_shelf.index()].empty {
                let following = self.shelves[next_shelf.index()].next;
                self.shelves[shelf_slot.index()].height += self.shelves[next_shelf.index()].height;
                self.shelves[shelf_slot.index()].next = following;
                if following.is_some() {
                    self.shelves[following.index()].previous = shelf_slot;
                }
                self.remove_shelf(next_shelf);
            }

            let previous_shelf = self.shelves[shelf_slot.index()].previous;
            if previous_shelf.is_some() && self.shelves[previous_shelf.index()].empty {
                let following = self.shelves[shelf_slot.index()].next;
                self.shelves[previous_shelf.index()].height +=
                    self.shelves[shelf_slot.index()].height;
                self.shelves[previous_shelf.index()].next = following;
                if following.is_some() {
                    self.shelves[following.index()].previous = previous_shelf;
                }
                self.remove_shelf(shelf_slot);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.allocations == 0
    }

    fn add_item(&mut self, mut item: Item) -> Slot {
        if self.free_items.is_some() {
            let slot = self.free_items;
            self.free_items = self.items[slot.index()].next;
            item.generation = self.items[slot.index()].generation.wrapping_add(1);
            self.items[slot.index()] = item;
            slot
        } else {
            assert!(self.items.len() < u16::MAX as usize, "too many atlas items");
            let slot = Slot(self.items.len() as u16);
            self.items.push(item);
            slot
        }
    }

    fn remove_item(&mut self, slot: Slot) {
        self.items[slot.index()].next = self.free_items;
        self.free_items = slot;
    }

    fn remove_shelf(&mut self, slot: Slot) {
        self.remove_item(self.shelves[slot.index()].first_free);
        self.shelves[slot.index()].next = self.free_shelves;
        self.free_shelves = slot;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reclaims_fragmented_space() {
        #[derive(Clone, Copy)]
        struct Live {
            id: AllocId,
            position: [u32; 2],
            size: [u32; 2],
        }

        let mut atlas = AtlasAllocator::new([256, 256]);
        let mut live: Vec<Live> = Vec::new();
        let mut state = 0x1234_5678_u32;
        let mut random = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            state
        };

        for _ in 0..10_000 {
            if !live.is_empty() && (live.len() >= 64 || random() & 1 == 0) {
                let index = random() as usize % live.len();
                atlas.deallocate(live.swap_remove(index).id);
                continue;
            }

            let size = [random() % 48 + 1, random() % 48 + 1];
            let Some(allocation) = atlas.allocate(size) else {
                continue;
            };
            assert!(allocation.position[0] + size[0] <= 256);
            assert!(allocation.position[1] + size[1] <= 256);
            assert!(live.iter().all(|other| {
                allocation.position[0] + size[0] <= other.position[0]
                    || other.position[0] + other.size[0] <= allocation.position[0]
                    || allocation.position[1] + size[1] <= other.position[1]
                    || other.position[1] + other.size[1] <= allocation.position[1]
            }));
            live.push(Live {
                id: allocation.id,
                position: allocation.position,
                size,
            });
        }

        for allocation in live {
            atlas.deallocate(allocation.id);
        }

        assert!(atlas.is_empty());
        let full = atlas.allocate([256, 256]).unwrap();
        assert_eq!(full.position, [0, 0]);
    }

    #[test]
    fn reuses_evicted_glyph_space() {
        let mut atlas = AtlasAllocator::new([64, 32]);
        let a = atlas.allocate([16, 16]).unwrap();
        let b = atlas.allocate([16, 16]).unwrap();
        let c = atlas.allocate([16, 16]).unwrap();
        assert_eq!(a.position, [0, 0]);
        assert_eq!(b.position, [16, 0]);
        assert_eq!(c.position, [32, 0]);

        atlas.deallocate(b.id);
        let replacement = atlas.allocate([16, 16]).unwrap();
        assert_eq!(replacement.position, [16, 0]);
    }
}
