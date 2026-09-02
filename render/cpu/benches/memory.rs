use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
};

use blit::{LogicalRect, PhysicalRect, Scale2};
use blit_cpu::{
    FontData, FontFace, Renderer, RendererConfig, Scanline, TextLayoutEngine, VecBuffer, Xrgb8888,
    color::Color,
    command_list::{ClipId, CommandList, Rectangle},
    text_types::FontId,
};

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static GROSS: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn main() {
    const SIDE: usize = 32;
    let mut commands = CommandList::default();
    for index in 0..SIDE * SIDE {
        let area = LogicalRect {
            x: (index % SIDE) as f32,
            y: (index / SIDE) as f32,
            width: 1.0,
            height: 1.0,
        };
        commands.push_rectangle(
            Rectangle::new(area).background(Color::BLACK),
            area.to_physical(Scale2::IDENTITY),
            ClipId::default(),
        );
    }
    let damage = [PhysicalRect {
        x: 0,
        y: 0,
        width: SIDE as i32,
        height: SIDE as i32,
    }];
    let mut text: Box<dyn TextLayoutEngine> =
        Box::new(blit_text_cosmic::Backend::without_system_fonts());
    let font = text
        .register_font(FontData::Static(include_bytes!(env!("BLIT_TEST_FONT"))), 0)
        .unwrap();
    let mut renderer = Renderer::new(
        VecBuffer::<Xrgb8888>::new(SIDE, SIDE),
        RendererConfig {
            fonts: vec![FontFace {
                id: FontId::default(),
                weight: 400,
                font,
            }],
            text_cache_capacity: 1,
            layout_cache_capacity: 1,
            glyph_cache_capacity: 1,
            shadow_cache_capacity: 0,
        },
        text,
    )
    .strategy(Scanline::default());
    let baseline = CURRENT.load(Relaxed);
    GROSS.store(0, Relaxed);
    renderer.render(&commands, &damage);
    let warm = GROSS.load(Relaxed);
    GROSS.store(0, Relaxed);
    renderer.render(&commands, &damage);
    let retained = CURRENT.load(Relaxed) - baseline;
    let steady = GROSS.load(Relaxed);

    println!("CPU retained render memory after 1,024 rectangles");
    println!("retained: {retained} B");
    println!("warm allocations: {warm} B");
    println!("steady allocations: {steady} B");
}

struct TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            CURRENT.fetch_add(layout.size(), Relaxed);
            GROSS.fetch_add(layout.size(), Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        CURRENT.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        if !pointer.is_null() {
            CURRENT.fetch_sub(layout.size(), Relaxed);
            CURRENT.fetch_add(size, Relaxed);
            GROSS.fetch_add(size, Relaxed);
        }
        pointer
    }
}
