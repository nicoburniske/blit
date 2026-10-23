use std::{hint::black_box, time::Duration};

use blit::{Atom, AtomCx, Constraints, Frame, FrameInfo, Input, Rect, Size, Sizing, Ui};
use blit_layout::flex;

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main()
}

#[divan::bench(args = ["fit", "grow", "capped"])]
fn flex_layout(bencher: divan::Bencher, sizing: &str) {
    struct Child;
    impl Atom<()> for Child {
        fn measure(&self, _: &mut AtomCx<'_, ()>, bounds: Constraints<f32>) -> Size<f32> {
            bounds.constrain(Size::new(8.0, 2.0))
        }
        fn paint(&self, _: &mut AtomCx<'_, ()>, _: Rect<f32>) {}
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
