use std::{hint::black_box, time::Duration};

use blit::{
    Animation, Constraints, Context, Easing, FrameInfo, Interact, InteractCx, Kernel, Layout, LayoutCx, Motion, Point,
    Scope, Size, Spring, Tween, Ui, input::Input, state,
};

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main()
}

#[divan::bench(args = [100, 1000])]
fn animate_tweens(bencher: divan::Bencher, count: usize) {
    animate(bencher, count, Tween::new(1.0, Easing::EASE_OUT_QUAD).into());
}

#[divan::bench(args = [100, 1000])]
fn animate_springs(bencher: divan::Bencher, count: usize) {
    animate(bencher, count, Spring::new(0.250, 0.2).into());
}

#[divan::bench(args = [100, 1000])]
fn animate_critical_springs(bencher: divan::Bencher, count: usize) {
    animate(bencher, count, Spring::new(0.250, 0.0).into());
}

#[divan::bench(args = [100, 1000])]
fn motion_tween_position(bencher: divan::Bencher, count: usize) {
    motions(bencher, count, Tween::new(1.0, Easing::EASE_OUT_QUAD).into(), false);
}

#[divan::bench(args = [100, 1000])]
fn motion_spring_position(bencher: divan::Bencher, count: usize) {
    motions(bencher, count, Spring::new(0.250, 0.2).into(), false);
}

#[divan::bench(args = [100, 1000])]
fn motion_tween_size(bencher: divan::Bencher, count: usize) {
    motions(bencher, count, Tween::new(1.0, Easing::EASE_OUT_QUAD).into(), true);
}

#[divan::bench(args = [100, 1000])]
fn motion_spring_size(bencher: divan::Bencher, count: usize) {
    motions(bencher, count, Spring::new(0.250, 0.2).into(), true);
}

#[divan::bench(args = [100, 1000])]
fn build_passive(bencher: divan::Bencher, count: usize) {
    let mut kernel = Kernel::default();
    bencher.bench_local(|| {
        build(&mut kernel, count, |mut ui, _| {
            ui.insert(());
        })
    });
}

#[divan::bench(args = [100, 1000])]
fn build_explicit_ids(bencher: divan::Bencher, count: usize) {
    let mut kernel = Kernel::default();
    bencher.bench_local(|| {
        kernel.build(
            &mut (),
            FrameInfo::new(Size::uniform(1000.0)),
            Duration::ZERO,
            Input::None,
            |ui: Ui<'_, ()>| {
                let mut root = ui.layout(Stack::default());
                for index in 0..count {
                    root.child_in(Scope::ROOT, index).insert(());
                }
            },
        );
        black_box(&kernel);
    });
}

#[divan::bench(args = [100, 1000])]
fn build_interactive(bencher: divan::Bencher, count: usize) {
    let mut kernel = Kernel::default();
    bencher.bench_local(|| {
        build(&mut kernel, count, |mut ui, _| {
            black_box(ui.interact(Hover));
        })
    });
}

#[divan::bench(args = [100, 1000])]
fn build_motions(bencher: divan::Bencher, count: usize) {
    let mut kernel = Kernel::default();
    bencher.bench_local(|| {
        build(&mut kernel, count, |ui, _| {
            ui.motion(Motion::new(Tween::new(1.0, Easing::LINEAR))).insert(());
        })
    });
}

#[divan::bench(args = [100, 1000])]
fn build_named_parent(bencher: divan::Bencher, count: usize) {
    let bench_root = 1000;
    let mut kernel = Kernel::default();
    bencher.bench_local(|| {
        kernel.build(
            &mut (),
            FrameInfo::new(Size::uniform(1000.0)),
            Duration::ZERO,
            Input::None,
            |ui: Ui<'_, ()>| {
                let mut outer = ui.layout(Stack::default());
                let mut root = outer.child_in(Scope::ROOT, bench_root).layout(Stack::default());
                let target = Scope::ROOT.key(bench_root);
                for _ in 0..count {
                    root.child().parent(target).insert(());
                }
            },
        );
        black_box(&kernel);
    });
}

#[divan::bench(args = [100, 1000])]
fn build_and_layout(bencher: divan::Bencher, count: usize) {
    let mut kernel = Kernel::default();
    bencher.bench_local(|| {
        build(&mut kernel, count, |mut ui, _| {
            ui.insert(());
        });
        kernel.layout(&mut ());
        black_box(&kernel);
    });
}

#[divan::bench(args = [100, 1000])]
fn build_interactive_and_layout(bencher: divan::Bencher, count: usize) {
    let mut kernel = Kernel::default();
    bencher.bench_local(|| {
        build(&mut kernel, count, |mut ui, _| {
            black_box(ui.interact(Hover));
        });
        kernel.layout(&mut ());
        black_box(&kernel);
    });
}

#[divan::bench]
fn build_interactive_after_peak(bencher: divan::Bencher) {
    let mut kernel = Kernel::default();
    build(&mut kernel, 100_000, |mut ui, _| {
        black_box(ui.interact(Hover));
    });
    kernel.layout(&mut ());
    bencher.bench_local(|| {
        build(&mut kernel, 100, |mut ui, _| {
            black_box(ui.interact(Hover));
        });
        kernel.layout(&mut ());
        black_box(&kernel);
    });
}

#[divan::bench(args = [1000])]
fn build_areas_and_layout(bencher: divan::Bencher, count: usize) {
    let mut kernel = Kernel::default();
    bencher.bench_local(|| {
        kernel.build(
            &mut (),
            FrameInfo::new(Size::uniform(1000.0)),
            Duration::ZERO,
            Input::None,
            |mut ui: Ui<'_, ()>| {
                for index in 0..10 {
                    black_box(ui.node(Scope::ROOT.key(index)).area());
                }
                let mut root = ui.layout(Stack::default());
                for index in 0..count {
                    root.child_in(Scope::ROOT, index).insert(());
                }
            },
        );
        kernel.layout(&mut ());
        black_box(&kernel);
    });
}

fn build(kernel: &mut Kernel<()>, count: usize, mut child: impl for<'a> FnMut(Ui<'a, (), state::Child<()>>, usize)) {
    kernel.build(
        &mut (),
        FrameInfo::new(Size::uniform(1000.0)),
        Duration::ZERO,
        Input::None,
        |ui: Ui<'_, ()>| {
            let mut root = ui.layout(Stack::default());
            for index in 0..count {
                child(root.child(), index);
            }
        },
    );
    black_box(kernel);
}

fn animate(bencher: divan::Bencher, count: usize, animation: Animation) {
    let mut kernel = Kernel::default();
    let mut elapsed = Duration::ZERO;
    let mut render = |elapsed: Duration, target: f32| {
        kernel.build(
            &mut (),
            FrameInfo::new(Size::uniform(1000.0)),
            elapsed,
            Input::None,
            |mut ui: Ui<'_, ()>| {
                for index in 0..count {
                    black_box(
                        ui.node(Scope::ROOT.key(index))
                            .animate(target, animation, Default::default()),
                    );
                }
            },
        );
        kernel.layout(&mut ());
        black_box(&kernel);
    };
    render(elapsed, 0.0);
    render(elapsed, 100.0);
    bencher.bench_local(|| {
        elapsed += Duration::from_millis(1);
        render(elapsed, if elapsed.as_millis() % 1000 < 500 { 100.0 } else { 0.0 });
    });
}

fn motions(bencher: divan::Bencher, count: usize, animation: Animation, size: bool) {
    let mut kernel = Kernel::default();
    let mut elapsed = Duration::ZERO;
    let config = Motion {
        animation,
        restart: false,
        delay: 0.0,
        lift: None,
        properties: if size {
            blit::MotionProps::SIZE
        } else {
            blit::MotionProps::POSITION
        },
    };
    let mut render = |elapsed: Duration, target: f32| {
        kernel.build(
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
                    root.child_in(Scope::ROOT, index).motion(config).insert(());
                }
            },
        );
        kernel.layout(&mut ());
        black_box(&kernel);
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
