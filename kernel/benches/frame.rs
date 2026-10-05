use std::{hint::black_box, time::Duration};

use blit::{
    Atom, Constraints, Frame, FrameInfo, Input, Layout, LayoutCx, Point, Rect, Sense, Size, Transition, Ui, WidgetId,
    state,
};
use blit_layout::{Sizing, flex};

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main()
}

#[divan::bench(args = [100, 1000])]
fn build_passive(bencher: divan::Bencher, count: usize) {
    let mut frame = Frame::default();
    bencher.bench_local(|| {
        build(&mut frame, count, |mut ui, _| {
            ui.insert(());
        })
    });
}

#[divan::bench(args = [100, 1000])]
fn build_explicit_ids(bencher: divan::Bencher, count: usize) {
    let mut frame = Frame::default();
    bencher.bench_local(|| {
        build(&mut frame, count, |ui, index| {
            ui.widget_id(WidgetId::new(index)).insert(());
        })
    });
}

#[divan::bench(args = [100, 1000])]
fn build_interactive(bencher: divan::Bencher, count: usize) {
    let mut frame = Frame::default();
    bencher.bench_local(|| {
        build(&mut frame, count, |mut ui, _| {
            black_box(ui.interact(Sense::CLICK));
        })
    });
}

#[divan::bench(args = [100, 1000])]
fn build_transitions(bencher: divan::Bencher, count: usize) {
    let mut frame = Frame::default();
    bencher.bench_local(|| {
        build(&mut frame, count, |ui, _| {
            ui.transition(Transition::new(Duration::from_secs(1))).insert(());
        })
    });
}

#[divan::bench(args = [100, 1000])]
fn build_named_parent(bencher: divan::Bencher, count: usize) {
    let mut frame = Frame::default();
    let id = WidgetId::new("bench root");
    bencher.bench_local(|| {
        build(&mut frame, count, |ui, _| {
            ui.parent(id).insert(());
        })
    });
}

#[divan::bench(args = [100, 1000])]
fn build_and_layout(bencher: divan::Bencher, count: usize) {
    let mut frame = Frame::default();
    bencher.bench_local(|| {
        build(&mut frame, count, |mut ui, _| {
            ui.insert(());
        });
        frame.layout(&mut ());
        black_box(&frame);
    });
}

#[divan::bench(args = [1000])]
fn build_interactive_and_layout(bencher: divan::Bencher, count: usize) {
    let mut frame = Frame::default();
    bencher.bench_local(|| {
        build(&mut frame, count, |mut ui, _| {
            black_box(ui.interact(Sense::CLICK));
        });
        frame.layout(&mut ());
        black_box(&frame);
    });
}

#[divan::bench(args = [1000])]
fn build_geometry_and_layout(bencher: divan::Bencher, count: usize) {
    let mut frame = Frame::default();
    bencher.bench_local(|| {
        frame.build(
            &mut (),
            FrameInfo::new(Size::uniform(1000.0)),
            Duration::ZERO,
            Input::None,
            |mut ui: Ui<'_, ()>| {
                for index in 0..10 {
                    black_box(ui.geometry(WidgetId::new(index)));
                }
                let mut root = ui.layout(Stack);
                for index in 0..count {
                    root.child().widget_id(WidgetId::new(index)).insert(());
                }
            },
        );
        frame.layout(&mut ());
        black_box(&frame);
    });
}

#[divan::bench(args = ["fit", "grow", "capped"])]
fn flex_layout(bencher: divan::Bencher, sizing: &str) {
    struct Child;
    impl Atom<()> for Child {
        fn measure(&self, _: &mut (), bounds: Constraints) -> Size {
            bounds.constrain(Size::new(8.0, 2.0))
        }
        fn paint(&self, _: &mut (), _: Rect) {}
        fn paint_bounds(&self, area: Rect) -> Rect {
            area
        }
    }
    let mut frame = Frame::default();
    frame.build(
        &mut (),
        FrameInfo::new(Size::new(16000.0, 32.0)),
        Duration::ZERO,
        Input::None,
        |ui: Ui<'_, ()>| {
            let mut root = ui.layout(flex::row());
            for index in 0..1000 {
                let width = match sizing {
                    "fit" => Sizing::fit(),
                    "grow" => Sizing::grow(),
                    "capped" => Sizing::grow_range(0.0, 8.0 + (index % 3) as f32 * 8.0),
                    _ => unreachable!(),
                };
                root.child().item(flex::item().width(width)).insert(Child);
            }
        },
    );
    for _ in 0..4 {
        frame.layout(&mut ());
    }
    bencher.bench_local(|| {
        frame.layout(&mut ());
        black_box(&frame);
    });
}

fn build(frame: &mut Frame<()>, count: usize, mut child: impl for<'a> FnMut(Ui<'a, (), state::Child<()>>, usize)) {
    frame.build(
        &mut (),
        FrameInfo::new(Size::uniform(1000.0)),
        Duration::ZERO,
        Input::None,
        |ui: Ui<'_, ()>| {
            let mut root = ui.widget_id(WidgetId::new("bench root")).layout(Stack);
            for index in 0..count {
                child(root.child(), index);
            }
        },
    );
    black_box(frame);
}

struct Stack;

impl Layout<()> for Stack {
    type Item = ();

    fn layout(&self, cx: &mut LayoutCx<'_, (), ()>, bounds: Constraints) -> Size {
        for child in cx.children() {
            cx.layout_child(child, Constraints::tight(Size::ZERO));
            cx.set_child_position(child, Point::ZERO);
        }
        bounds.min
    }
}
