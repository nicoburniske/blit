use std::{
    alloc::{GlobalAlloc, Layout as AllocLayout, System},
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
    time::Duration,
};

use blit::{
    FrameGraphMemory, Ui, UiState,
    color::Color,
    container::Sizing,
    geometry::{LogicalInsets, PhysicalRect},
    input::Input,
    render,
    style::Clip,
    widget::Text,
};

use support::{NoopRenderer, command_frame};

#[allow(dead_code)]
mod support;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static GROSS: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn main() {
    let representative = measure("representative", 104, representative_frame);
    let benchmark = measure("core benchmark", 1026, command_frame);

    println!(
        "{:<16} {:>6} {:>6} {:>8} {:>12} {:>12} {:>12} {:>10}",
        "scene", "nodes", "node B", "capacity", "graph heap", "core heap", "warm alloc", "steady",
    );
    for report in [representative, benchmark] {
        println!(
            "{:<16} {:>6} {:>6} {:>8} {:>9.1} KiB {:>9.1} KiB {:>9.1} KiB {:>7} B",
            report.name,
            report.nodes,
            report.graph.node_size,
            report.graph.node_capacity,
            report.graph.heap_bytes as f64 / 1024.0,
            report.core_heap as f64 / 1024.0,
            report.growth as f64 / 1024.0,
            report.steady,
        );
    }
}

#[derive(Clone, Copy)]
struct Report {
    name: &'static str,
    nodes: usize,
    graph: FrameGraphMemory,
    core_heap: usize,
    growth: usize,
    steady: usize,
}

fn measure(name: &'static str, nodes: usize, frame: fn(&mut Ui)) -> Report {
    let baseline = CURRENT.load(Relaxed);
    let mut renderer = NoopRenderer;
    let screen = PhysicalRect {
        x: 0,
        y: 0,
        width: 1280,
        height: 8192,
    };
    let mut state = UiState::new(screen, blit::RepaintBuffer::Reused, 1.0);
    GROSS.store(0, Relaxed);
    render(
        &mut renderer,
        &mut state,
        Duration::ZERO,
        [Input::None],
        frame,
    );
    render(
        &mut renderer,
        &mut state,
        Duration::ZERO,
        [Input::None],
        frame,
    );
    let growth = GROSS.load(Relaxed);

    GROSS.store(0, Relaxed);
    render(
        &mut renderer,
        &mut state,
        Duration::ZERO,
        [Input::None],
        frame,
    );
    let report = Report {
        name,
        nodes,
        graph: state.frame_graph_memory(),
        core_heap: CURRENT.load(Relaxed) - baseline,
        growth,
        steady: GROSS.load(Relaxed),
    };
    drop((renderer, state));
    assert_eq!(CURRENT.load(Relaxed), baseline);
    report
}

fn representative_frame(ui: &mut Ui) {
    let mut root = ui
        .container()
        .col()
        .width(Sizing::grow())
        .padding(LogicalInsets::uniform(8.0))
        .gap(8.0)
        .open();
    {
        let mut header = root
            .container()
            .row()
            .width(Sizing::grow())
            .height(Sizing::fixed(48.0))
            .background(Color::from_rgba8(30, 35, 45, 255))
            .open();
        header.add(Text::new("dashboard"));
    }
    {
        let mut body = root
            .container()
            .row()
            .width(Sizing::grow())
            .height(Sizing::grow())
            .gap(12.0)
            .clip(Clip::Bounds)
            .open();
        {
            let mut sidebar = body
                .container()
                .col()
                .width(Sizing::fixed(180.0))
                .height(Sizing::grow())
                .gap(6.0)
                .open();
            for _ in 0..8 {
                sidebar.add(Text::new("navigation"));
            }
        }
        {
            let mut main = body
                .container()
                .col()
                .width(Sizing::grow())
                .height(Sizing::grow())
                .gap(8.0)
                .open();
            for _ in 0..8 {
                let mut card = main
                    .container()
                    .col()
                    .width(Sizing::grow())
                    .gap(4.0)
                    .background(Color::from_rgba8(240, 240, 244, 255))
                    .open();
                card.add(Text::new("card title"));
                for _ in 0..3 {
                    let mut row = card.container().row().width(Sizing::grow()).gap(8.0).open();
                    row.add(Text::new("label"));
                    row.add(Text::new("value"));
                }
            }
        }
    }
    root.add(Text::new("status"));
}

struct TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: AllocLayout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            CURRENT.fetch_add(layout.size(), Relaxed);
            GROSS.fetch_add(layout.size(), Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: AllocLayout) {
        CURRENT.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: AllocLayout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        if !pointer.is_null() {
            CURRENT.fetch_sub(layout.size(), Relaxed);
            CURRENT.fetch_add(size, Relaxed);
            GROSS.fetch_add(size, Relaxed);
        }
        pointer
    }
}
