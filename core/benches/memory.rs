use std::{
    alloc::{GlobalAlloc, Layout as AllocLayout, System},
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
    time::Duration,
};

use blit::{
    FrameGraphMemory, Ui, UiState,
    color::Color,
    container::Sizing,
    geometry::Sides,
    input::Input,
    layout::Flex,
    render,
    repaint::{IncrementalRepaint, MyersTracker},
    style::{Clip, Style},
    widget::Text,
};

use support::{NoopRenderer, command_frame, layer_frame, z_index_frame};

#[allow(dead_code)]
mod support;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static GROSS: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn main() {
    let representative = measure("representative", 104, representative_frame);
    let benchmark = measure("core benchmark", 1026, command_frame);
    let z_index = measure("z-index", 1025, z_index_frame);
    let layers = measure("layers", 1025, layer_frame);

    println!(
        "{:<16} {:>6} {:>6} {:>8} {:>12} {:>12} {:>12} {:>10}",
        "scene", "nodes", "node B", "capacity", "graph heap", "core heap", "warm alloc", "steady",
    );
    for report in [representative, benchmark, z_index, layers] {
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
    let mut state = UiState::default();
    let mut repaint = IncrementalRepaint::new(MyersTracker::default(), false);
    GROSS.store(0, Relaxed);
    render(
        &mut renderer,
        &mut state,
        &mut repaint,
        Duration::ZERO,
        [Input::None],
        frame,
    );
    render(
        &mut renderer,
        &mut state,
        &mut repaint,
        Duration::ZERO,
        [Input::None],
        frame,
    );
    let growth = GROSS.load(Relaxed);

    GROSS.store(0, Relaxed);
    render(
        &mut renderer,
        &mut state,
        &mut repaint,
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
    drop((renderer, state, repaint));
    assert_eq!(CURRENT.load(Relaxed), baseline);
    report
}

fn representative_frame(ui: &mut Ui) {
    let mut root = ui
        .layout(Flex::column().padding(Sides::all(8.0)).gap(8.0))
        .width(Sizing::grow())
        .open();
    root.add(|ui: &mut Ui| {
        let mut header = ui
            .layout(Flex::row())
            .width(Sizing::grow())
            .height(Sizing::fixed(48.0))
            .style(Style::new().background(Color::from_rgba8(30, 35, 45, 255)))
            .open();
        header.add(Text::new("dashboard"));
    });
    root.add(|ui: &mut Ui| {
        let mut body = ui
            .layout(Flex::row().gap(12.0))
            .width(Sizing::grow())
            .height(Sizing::grow())
            .clip(Clip::Bounds)
            .open();
        body.add(|ui: &mut Ui| {
            let mut sidebar = ui
                .layout(Flex::column().gap(6.0))
                .width(Sizing::fixed(180.0))
                .height(Sizing::grow())
                .open();
            for _ in 0..8 {
                sidebar.add(Text::new("navigation"));
            }
        });
        body.add(|ui: &mut Ui| {
            let mut main = ui
                .layout(Flex::column().gap(8.0))
                .width(Sizing::grow())
                .height(Sizing::grow())
                .open();
            for _ in 0..8 {
                main.add(|ui: &mut Ui| {
                    let mut card = ui
                        .layout(Flex::column().gap(4.0))
                        .width(Sizing::grow())
                        .style(Style::new().background(Color::from_rgba8(240, 240, 244, 255)))
                        .open();
                    card.add(Text::new("card title"));
                    for _ in 0..3 {
                        card.add(|ui: &mut Ui| {
                            let mut row =
                                ui.layout(Flex::row().gap(8.0)).width(Sizing::grow()).open();
                            row.add(Text::new("label"));
                            row.add(Text::new("value"));
                        });
                    }
                });
            }
        });
    });
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
