use blit::{Constraints, LogicalPoint, LogicalSize};

use crate::{Offset, Sizing, round, sizing_range};

blit::builder! {
    /// positions one child relative to the target selected by `Ui::target`
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout {
        new(),
        x: Offset = 0 as Offset,
        y: Offset = 0 as Offset,
        target_anchor: Anchor = Anchor::TopLeft,
        child_anchor: Anchor = Anchor::TopLeft,
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl<C> blit::Layout<C> for Layout {
    type Item = ();

    fn layout(&self, cx: &mut blit::LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> LogicalSize {
        let mut children = cx.children();
        let child = children.next().expect("absolute requires one flow child");
        assert!(children.next().is_none(), "absolute accepts one flow child");

        let containing = cx.size(cx.visual_parent());
        let range = |sizing: Sizing, available: f32| {
            let sizing = sizing.into_float();
            let available = round(available);
            if matches!(sizing, Sizing::Grow { .. }) {
                let size = sizing.clamp(available);
                (size, size)
            } else {
                sizing_range(sizing, available)
            }
        };
        let width = range(self.width, containing.width);
        let height = range(self.height, containing.height);
        let size = cx.layout_child(
            child,
            Constraints {
                min: bounds.constrain(LogicalSize::new(round(width.0), round(height.0))),
                max: bounds.constrain(LogicalSize::new(round(width.1), round(height.1))),
            },
        );
        let target = cx.size(cx.parent());
        let anchor = |anchor| match anchor {
            Anchor::TopLeft => LogicalPoint::new(0.0, 0.0),
            Anchor::Top => LogicalPoint::new(0.5, 0.0),
            Anchor::TopRight => LogicalPoint::new(1.0, 0.0),
            Anchor::Left => LogicalPoint::new(0.0, 0.5),
            Anchor::Center => LogicalPoint::new(0.5, 0.5),
            Anchor::Right => LogicalPoint::new(1.0, 0.5),
            Anchor::BottomLeft => LogicalPoint::new(0.0, 1.0),
            Anchor::Bottom => LogicalPoint::new(0.5, 1.0),
            Anchor::BottomRight => LogicalPoint::new(1.0, 1.0),
        };
        let target_anchor = anchor(self.target_anchor);
        let child_anchor = anchor(self.child_anchor);
        cx.set_position(child, LogicalPoint::ZERO);
        cx.set_position(
            cx.node(),
            LogicalPoint::new(
                round(round(target.width) * target_anchor.x - round(size.width) * child_anchor.x + self.x as f32),
                round(round(target.height) * target_anchor.y - round(size.height) * child_anchor.y + self.y as f32),
            ),
        );
        bounds.constrain(size)
    }
}
