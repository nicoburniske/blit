use std::{
    alloc::{GlobalAlloc, Layout, System},
    any::Any,
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

use blit::{
    PhysicalRect,
    internal::bump::{Bump, Offset},
};
use blit_gui::display_list::{ClipId, Command, DisplayList};

#[global_allocator]
static ALLOCATOR: Counted = Counted;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

fn main() {
    const COUNT: usize = 1024;
    const FRAMES: usize = 1000;
    let bounds = PhysicalRect::new(0, 0, 10, 10);
    let mut erased = DisplayList::default();
    let mut typed = TypedList::<u64> {
        bump: Bump::default(),
        commands: Vec::new(),
    };
    let mut boxed: Vec<BoxedRecord> = Vec::new();
    for index in 0..COUNT {
        erased.push_custom(bounds, ClipId::default(), |_| index as u64);
        typed.commands.push(TypedRecord {
            bounds,
            clip: ClipId::default(),
            value: typed.bump.push(index as u64),
        });
        boxed.push(BoxedRecord {
            bounds,
            clip: ClipId::default(),
            value: Box::new(index as u64),
        });
    }
    erased.clear();
    typed.clear();
    boxed.clear();

    let mut checksum = 0;
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    let start = Instant::now();
    for _ in 0..FRAMES {
        for index in 0..COUNT {
            erased.push_custom(bounds, ClipId::default(), |_| black_box(index as u64));
        }
        for record in erased.iter() {
            black_box((record.bounds, record.clip));
            let Command::Custom(command) = record.command else {
                unreachable!()
            };
            checksum += black_box(*command.get::<u64>().unwrap());
        }
        erased.clear();
    }
    let elapsed = start.elapsed();
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    println!("erased bump: {allocations} warmed allocations, {elapsed:?}, checksum {checksum}");

    let before = ALLOCATIONS.load(Ordering::Relaxed);
    let start = Instant::now();
    for _ in 0..FRAMES {
        for index in 0..COUNT {
            typed.commands.push(TypedRecord {
                bounds,
                clip: ClipId::default(),
                value: typed.bump.push(black_box(index as u64)),
            });
        }
        for command in &typed.commands {
            black_box((command.bounds, command.clip));
            // safety: typed entries refer to initialized values in this bump epoch
            checksum += black_box(unsafe { *typed.bump.get(command.value) });
        }
        typed.clear();
    }
    let elapsed = start.elapsed();
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    println!("typed bump: {allocations} warmed allocations, {elapsed:?}, checksum {checksum}");

    let before = ALLOCATIONS.load(Ordering::Relaxed);
    let start = Instant::now();
    for _ in 0..FRAMES {
        for index in 0..COUNT {
            boxed.push(BoxedRecord {
                bounds,
                clip: ClipId::default(),
                value: Box::new(black_box(index as u64)),
            });
        }
        for command in &boxed {
            black_box((command.bounds, command.clip));
            checksum += black_box(*command.value.downcast_ref::<u64>().unwrap());
        }
        boxed.clear();
    }
    let elapsed = start.elapsed();
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    println!("boxed values: {allocations} warmed allocations, {elapsed:?}, checksum {checksum}");
}

struct TypedList<C> {
    bump: Bump,
    commands: Vec<TypedRecord<C>>,
}

struct TypedRecord<C> {
    bounds: PhysicalRect,
    clip: ClipId,
    value: Offset<C>,
}
struct BoxedRecord {
    bounds: PhysicalRect,
    clip: ClipId,
    value: Box<dyn Any>,
}

impl<C: Copy> TypedList<C> {
    fn clear(&mut self) {
        self.commands.clear();
        self.bump.clear();
    }
}

struct Counted;

unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // safety: the allocation request is forwarded unchanged
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // safety: the allocation and its original layout are forwarded unchanged
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // safety: the allocation request is forwarded unchanged
        unsafe { System.realloc(pointer, layout, size) }
    }
}
