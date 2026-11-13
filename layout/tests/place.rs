use std::time::Duration;

use blit::{Atom, AtomCx, Constraints, Context, Frame, FrameInfo, Id, Input, NodeTarget, Rect, Size, Ui};
use blit_layout::{Align, Justify, absolute, absolute::Anchor, flex};

#[derive(Default)]
struct Cells(Vec<Rect<i32>>);

impl Context for Cells {
    type Scalar = i32;
}

struct BoxAtom;

impl Atom<Cells> for BoxAtom {
    fn measure(&self, _: &mut AtomCx<'_, Cells>, bounds: Constraints<i32>) -> Size<i32> {
        bounds.constrain(Size::new(6, 2))
    }

    fn paint(&self, cx: &mut AtomCx<'_, Cells>, area: Rect<i32>) {
        cx.context_mut().0.push(area);
    }
}

#[test]
fn place_sizes_the_outer_box_of_a_padded_layout() {
    for (width, height, expected) in [
        (
            absolute::Sizing::fit(),
            absolute::Sizing::fit(),
            Rect::new(31, 15, 8, 4),
        ),
        (
            absolute::Sizing::fixed(12),
            absolute::Sizing::fixed(8),
            Rect::new(27, 11, 12, 8),
        ),
        (
            absolute::Sizing::percent(0.5),
            absolute::Sizing::percent(0.5),
            Rect::new(19, 9, 20, 10),
        ),
    ] {
        let panel = Id::new("panel");
        let text = Id::new("text");
        let mut frame = Frame::default();
        let mut context = Cells::default();
        frame.build(
            &mut context,
            FrameInfo::new(Size::new(40, 20)),
            Duration::ZERO,
            Input::None,
            |ui: Ui<'_, Cells>| {
                let mut root = ui.layout(());
                let mut toast = root
                    .child()
                    .id(panel)
                    .place(
                        NodeTarget::Parent,
                        absolute::place()
                            .target_anchor(absolute::Anchor::BottomRight)
                            .child_anchor(absolute::Anchor::BottomRight)
                            .x(-1)
                            .y(-1)
                            .width(width)
                            .height(height),
                    )
                    .layout(flex::column().pad(1));
                toast.insert(BoxAtom);
                toast.child().id(text).insert(BoxAtom);
            },
        );
        frame.layout(&mut context);
        assert_eq!(frame.area(panel), Some(expected));
        frame.paint(&mut context);
        assert_eq!(context.0[0], expected);
        assert_eq!(context.0[1], frame.area(text).unwrap());
        assert_eq!(frame.area(text), Some(Rect::new(expected.x + 1, expected.y + 1, 6, 2)));
    }
}

#[test]
fn place_flips_and_clamps_into_screen() {
    let (button, popup) = (Id::new("button"), Id::new("popup"));
    let mut frame = Frame::default();
    frame.build(
        &mut Cells::default(),
        FrameInfo::new(Size::new(20, 10)),
        Duration::ZERO,
        Input::None,
        |ui: Ui<'_, Cells>| {
            let mut root = ui.layout(flex::column().justify(Justify::End).align(Align::Start));
            root.child().id(button).insert(BoxAtom);
            root.child().id(popup).place(
                button,
                absolute::place()
                    .target_anchor(Anchor::BottomLeft)
                    .x(18)
                    .width(absolute::Sizing::fixed(4))
                    .height(absolute::Sizing::fixed(4))
                    .flip(true)
                    .clamp(true),
            );
        },
    );
    frame.layout(&mut Cells::default());
    assert_eq!(frame.area(button), Some(Rect::new(0, 8, 6, 2)));
    assert_eq!(frame.area(popup), Some(Rect::new(16, 4, 4, 4)));
}
