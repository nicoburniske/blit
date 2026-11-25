use std::{cell::Cell, hint::black_box, time::Duration};

use blit::{
    Constraints, Context, FrameInfo, Interact, InteractCx, Kernel, Layout, LayoutCx, Scope, Size, Ui, input::Input,
};

fn main() {
    divan::main()
}

const COUNT: u32 = 1000;

/// unkeyed children from one call site
#[divan::bench]
fn sites(bencher: divan::Bencher) {
    frames(bencher, |mut root, _| {
        for _ in 0..COUNT {
            root.child().insert(());
        }
    });
}

/// keyed children in build order
#[divan::bench]
fn keys(bencher: divan::Bencher) {
    frames(bencher, |mut root, _| {
        for index in 0..COUNT {
            root.child_key(Scope::ROOT.key(index)).insert(());
        }
    });
}

/// ten by ten by ten unkeyed nodes
#[divan::bench]
fn tree_sites(bencher: divan::Bencher) {
    frames(bencher, |mut root, _| {
        for _ in 0..10 {
            let mut a = root.child().layout(Stack);
            for _ in 0..10 {
                let mut b = a.child().layout(Stack);
                for _ in 0..10 {
                    b.child().insert(());
                }
            }
        }
    });
}

/// ten by ten by ten keyed nodes
#[divan::bench]
fn tree_keys(bencher: divan::Bencher) {
    frames(bencher, |mut root, _| {
        let scope = root.scope();
        for i in 0..10u32 {
            let mut a = root.child_key(scope.key(i)).layout(Stack);
            let scope = a.scope();
            for j in 0..10u32 {
                let mut b = a.child_key(scope.key(j)).layout(Stack);
                let scope = b.scope();
                for k in 0..10u32 {
                    b.child_key(scope.key(k)).insert(());
                }
            }
        }
    });
}

/// keyed children rotated by one each frame
#[divan::bench]
fn keys_rotating(bencher: divan::Bencher) {
    frames(bencher, |mut root, frame| {
        for index in 0..COUNT {
            root.child_key(Scope::ROOT.key((index + frame) % COUNT)).insert(());
        }
    });
}

/// a window of keys sliding by one each frame, like a scrolling list
#[divan::bench]
fn keys_scrolling(bencher: divan::Bencher) {
    frames(bencher, |mut root, frame| {
        for index in frame..frame + COUNT {
            root.child_key(Scope::ROOT.key(index)).insert(());
        }
    });
}

/// unkeyed children behind a sibling from another call site that comes and goes
#[divan::bench]
fn sites_toggling(bencher: divan::Bencher) {
    frames(bencher, |mut root, frame| {
        if frame % 2 == 0 {
            root.child().insert(());
        }
        for _ in 0..COUNT {
            root.child().insert(());
        }
    });
}

/// unkeyed children that switch to another call site every frame
// the branches are the same code at two call sites, which is the point
#[allow(clippy::if_same_then_else)]
#[divan::bench]
fn sites_replaced(bencher: divan::Bencher) {
    frames(bencher, |mut root, frame| {
        if frame % 2 == 0 {
            for _ in 0..COUNT {
                root.child().insert(());
            }
        } else {
            for _ in 0..COUNT {
                root.child().insert(());
            }
        }
    });
}

/// every keyed child read through its key before it is built, like a scroll thumb
#[divan::bench]
fn lookups(bencher: divan::Bencher) {
    frames(bencher, |mut root, _| {
        let scope = Scope::ROOT;
        for index in 0..COUNT {
            black_box(root.node(scope.key(index)).area());
        }
        for index in 0..COUNT {
            root.child_key(Scope::ROOT.key(index)).insert(());
        }
    });
}

/// keyed children with an interaction each
#[divan::bench]
fn interactive(bencher: divan::Bencher) {
    frames(bencher, |mut root, _| {
        for index in 0..COUNT {
            black_box(root.child_key(Scope::ROOT.key(index)).interact(Hover));
        }
    });
}

/// cards keyed in the root scope, a tenth of them moving between two columns each frame
#[divan::bench]
fn reparenting(bencher: divan::Bencher) {
    frames(bencher, |mut root, frame| {
        for column in 0..2 {
            let mut parent = root.child().layout(Stack);
            for index in 0..COUNT {
                let moved = index % 10 == 0 && frame % 2 == 1;
                if (index % 2 == column) != moved {
                    parent.child_key(Scope::ROOT.key(index)).insert(());
                }
            }
        }
    });
}

/// a thousand unkeyed children in the frames after one of a hundred thousand
#[divan::bench]
fn after_peak(bencher: divan::Bencher) {
    frames(bencher, |mut root, frame| {
        let count = if frame == 0 { 100 * COUNT } else { COUNT };
        for _ in 0..count {
            root.child().insert(());
        }
    });
}

/// keyed children beside as many unkeyed ones that come and go every frame
#[divan::bench]
fn keys_beside_churn(bencher: divan::Bencher) {
    frames(bencher, |mut root, frame| {
        for index in 0..COUNT / 2 {
            root.child_key(Scope::ROOT.key(index)).insert(());
        }
        if frame % 2 == 0 {
            for _ in 0..COUNT / 2 {
                root.child().insert(());
            }
        }
    });
}

/// builds and lays out one frame per iteration, passing the frame number
fn frames(bencher: divan::Bencher, build: impl for<'a> Fn(Ui<'a, (), blit::state::Open<Stack>>, u32)) {
    let mut kernel = Kernel::default();
    let frame = Cell::new(0);
    let render = |kernel: &mut Kernel<()>| {
        kernel.build(
            &mut (),
            FrameInfo::new(Size::uniform(1000.0)),
            Duration::ZERO,
            Input::None,
            |ui: Ui<'_, ()>| build(ui.layout(Stack), frame.get()),
        );
        kernel.layout(&mut ());
        frame.set(frame.get() + 1);
        black_box(&*kernel);
    };
    render(&mut kernel);
    render(&mut kernel);
    bencher.bench_local(|| render(&mut kernel));
}

struct Stack;

impl Layout<()> for Stack {
    type Item = ();

    fn layout(&self, cx: &mut LayoutCx<'_, (), ()>, bounds: Constraints<f32>) -> Size<f32> {
        for child in cx.children() {
            cx.layout_child(child, Constraints::tight(Size::ZERO));
        }
        bounds.min
    }
}

#[derive(Clone, Copy)]
struct Hover;

impl<C: Context> Interact<C> for Hover {
    type State = ();
    type Response = bool;

    fn update(&self, _: &mut (), cx: &mut InteractCx<'_, C::Scalar>) -> bool {
        cx.hovered()
    }
}
