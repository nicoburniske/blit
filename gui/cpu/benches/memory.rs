use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
};

use blit::{LogicalRect, Scale2};
use blit_cpu::{Renderer, RendererConfig, Scanline, VecBuffer, Xrgb8888};
use blit_gui::{
    FontData, FontFamily, GuiContext, TextConfig, TextSystem,
    color::Color,
    display_list::{ClipId, DisplayList, Rectangle},
    text::{FontId, TextPhases},
};

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static GROSS: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn main() {
    const SIDE: usize = 32;
    let mut display_list = DisplayList::default();
    for index in 0..SIDE * SIDE {
        let area = LogicalRect {
            x: (index % SIDE) as f32,
            y: (index / SIDE) as f32,
            width: 1.0,
            height: 1.0,
        };
        display_list.push_rectangle(
            Rectangle::new(area).background(Color::BLACK),
            area.to_physical(Scale2::IDENTITY),
            ClipId::default(),
        );
    }
    let mut renderer = Renderer::new(
        VecBuffer::<Xrgb8888>::new(SIDE, SIDE),
        RendererConfig {
            damage_tile_size: 32,
            paint_cache_capacity: 1,
            glyph_cache_capacity: 1,
            shadow_cache_capacity: 0,
            text_phases: TextPhases::Four,
        },
    )
    .strategy(Scanline::default());
    let text = TextSystem::new(
        TextConfig {
            fonts: vec![FontFamily {
                id: FontId::default(),
                fonts: vec![FontData::Static(include_bytes!(env!("BLIT_TEST_FONT")))],
            }],
            text_cache_capacity: 1,
            layout_cache_capacity: 1,
        },
        blit_text_cosmic::Backend::without_system_fonts(),
    )
    .unwrap();
    let mut gui = GuiContext::new(text);
    *gui.render_input().display_list = display_list;
    let baseline = CURRENT.load(Relaxed);
    GROSS.store(0, Relaxed);
    renderer.render(gui.render_input(), 0);
    let warm = GROSS.load(Relaxed);
    GROSS.store(0, Relaxed);
    renderer.render(gui.render_input(), 0);
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
