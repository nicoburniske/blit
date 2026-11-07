use std::{hint::black_box, time::Duration};

use blit::{
    Animation, Constraints, Context, Easing, Frame, FrameInfo, Id, Input, Interact, InteractCx, Layout, LayoutCx,
    Motion, Point, Size, Spring, Tween, Ui, state,
};

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main()
}

#[divan::bench(args = [100, 1000])]
fn animate_tweens(bencher: divan::Bencher, count: usize) {
    animate(
        bencher,
        count,
        Tween::new(Duration::from_secs(1), Easing::EASE_OUT_QUAD).into(),
    );
}

#[divan::bench(args = [100, 1000])]
fn animate_springs(bencher: divan::Bencher, count: usize) {
    animate(bencher, count, Spring::new(Duration::from_millis(250), 0.2).into());
}

#[divan::bench(args = [100, 1000])]
fn animate_critical_springs(bencher: divan::Bencher, count: usize) {
    animate(bencher, count, Spring::new(Duration::from_millis(250), 0.0).into());
}

#[divan::bench(args = [100, 1000])]
fn motion_tween_position(bencher: divan::Bencher, count: usize) {
    motions(
        bencher,
        count,
        Tween::new(Duration::from_secs(1), Easing::EASE_OUT_QUAD).into(),
        false,
    );
}

#[divan::bench(args = [100, 1000])]
fn motion_spring_position(bencher: divan::Bencher, count: usize) {
    motions(
        bencher,
        count,
        Spring::new(Duration::from_millis(250), 0.2).into(),
        false,
    );
}

#[divan::bench(args = [100, 1000])]
fn motion_tween_size(bencher: divan::Bencher, count: usize) {
    motions(
        bencher,
        count,
        Tween::new(Duration::from_secs(1), Easing::EASE_OUT_QUAD).into(),
        true,
    );
}

#[divan::bench(args = [100, 1000])]
fn motion_spring_size(bencher: divan::Bencher, count: usize) {
    motions(
        bencher,
        count,
        Spring::new(Duration::from_millis(250), 0.2).into(),
        true,
    );
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
            ui.id(Id::new(index)).insert(());
        })
    });
}

#[divan::bench(args = [100, 1000])]
fn build_interactive(bencher: divan::Bencher, count: usize) {
    let mut frame = Frame::default();
    bencher.bench_local(|| {
        build(&mut frame, count, |mut ui, _| {
            black_box(ui.interact(Hover));
        })
    });
}

#[divan::bench(args = [100, 1000])]
fn build_motions(bencher: divan::Bencher, count: usize) {
    let mut frame = Frame::default();
    bencher.bench_local(|| {
        build(&mut frame, count, |ui, _| {
            ui.motion(Motion::new(Tween::new(Duration::from_secs(1), Easing::LINEAR)))
                .insert(());
        })
    });
}

#[divan::bench(args = [100, 1000])]
fn build_named_parent(bencher: divan::Bencher, count: usize) {
    let mut frame = Frame::default();
    let id = Id::new("bench root");
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

#[divan::bench(args = [100, 1000])]
fn build_interactive_and_layout(bencher: divan::Bencher, count: usize) {
    let mut frame = Frame::default();
    bencher.bench_local(|| {
        build(&mut frame, count, |mut ui, _| {
            black_box(ui.interact(Hover));
        });
        frame.layout(&mut ());
        black_box(&frame);
    });
}

#[divan::bench]
fn build_interactive_after_peak(bencher: divan::Bencher) {
    let mut frame = Frame::default();
    build(&mut frame, 100_000, |mut ui, _| {
        black_box(ui.interact(Hover));
    });
    frame.layout(&mut ());
    bencher.bench_local(|| {
        build(&mut frame, 100, |mut ui, _| {
            black_box(ui.interact(Hover));
        });
        frame.layout(&mut ());
        black_box(&frame);
    });
}

#[divan::bench(args = [1000])]
fn build_areas_and_layout(bencher: divan::Bencher, count: usize) {
    let mut frame = Frame::default();
    bencher.bench_local(|| {
        frame.build(
            &mut (),
            FrameInfo::new(Size::uniform(1000.0)),
            Duration::ZERO,
            Input::None,
            |mut ui: Ui<'_, ()>| {
                for index in 0..10 {
                    black_box(ui.area(Id::new(index)));
                }
                let mut root = ui.layout(Stack::default());
                for index in 0..count {
                    root.child().id(Id::new(index)).insert(());
                }
            },
        );
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
            let mut root = ui.id(Id::new("bench root")).layout(Stack::default());
            for index in 0..count {
                child(root.child(), index);
            }
        },
    );
    black_box(frame);
}

fn animate(bencher: divan::Bencher, count: usize, animation: Animation) {
    let mut frame = Frame::default();
    let mut elapsed = Duration::ZERO;
    let mut render = |elapsed: Duration, target: f32| {
        frame.build(
            &mut (),
            FrameInfo::new(Size::uniform(1000.0)),
            elapsed,
            Input::None,
            |mut ui: Ui<'_, ()>| {
                for index in 0..count {
                    black_box(ui.animate(Id::new(index), target, animation, Default::default()));
                }
            },
        );
        frame.layout(&mut ());
        black_box(&frame);
    };
    render(elapsed, 0.0);
    render(elapsed, 100.0);
    bencher.bench_local(|| {
        elapsed += Duration::from_millis(1);
        render(elapsed, if elapsed.as_millis() % 1000 < 500 { 100.0 } else { 0.0 });
    });
}

fn motions(bencher: divan::Bencher, count: usize, animation: Animation, size: bool) {
    let mut frame = Frame::default();
    let mut elapsed = Duration::ZERO;
    let config = Motion {
        animation,
        restart: false,
        properties: if size {
            blit::MotionProps::SIZE
        } else {
            blit::MotionProps::POSITION
        },
    };
    let mut render = |elapsed: Duration, target: f32| {
        frame.build(
            &mut (),
            FrameInfo::new(Size::uniform(1000.0)),
            elapsed,
            Input::None,
            |ui: Ui<'_, ()>| {
                let mut root = ui.layout(Stack {
                    size: Size::uniform(if size { target } else { 10.0 }),
                    position: if size { Point::ZERO } else { Point::new(target, target) },
                });
                for index in 0..count {
                    root.child().id(Id::new(index)).motion(config).insert(());
                }
            },
        );
        frame.layout(&mut ());
        black_box(&frame);
    };
    render(elapsed, 10.0);
    render(elapsed, 100.0);
    bencher.bench_local(|| {
        elapsed += Duration::from_millis(1);
        render(elapsed, if elapsed.as_millis() % 1000 < 500 { 100.0 } else { 10.0 });
    });
}

#[derive(Default)]
struct Stack {
    size: Size<f32>,
    position: Point<f32>,
}

impl Layout<()> for Stack {
    type Item = ();

    fn layout(&self, cx: &mut LayoutCx<'_, (), ()>, bounds: Constraints<f32>) -> Size<f32> {
        for child in cx.children() {
            cx.layout_child(child, Constraints::tight(self.size));
            cx.set_child_position(child, self.position);
        }
        bounds.min
    }
}

/// whether the pointer is over this widget
#[derive(Clone, Copy)]
struct Hover;

impl<C: Context> Interact<C> for Hover {
    type State = ();
    type Response = bool;

    fn update(&self, _: &mut (), cx: &mut InteractCx<'_, C::Scalar>) -> bool {
        cx.hovered()
    }
}
