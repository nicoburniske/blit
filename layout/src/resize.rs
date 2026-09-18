use blit::{Axis, Constraints, Layout as LayoutTrait, LayoutCx, Platform, Point, Size};

#[derive(Clone, Copy)]
pub struct Layout {
    pub size: Size,
    pub minimum: Size,
    pub maximum: Size,
    pub grip_size: Size,
}

#[derive(Clone, Copy)]
pub enum Item {
    Content,
    Right,
    Bottom,
    Corner,
}

impl<P: Platform> LayoutTrait<P> for Layout {
    type Item = Item;

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, constraints: Constraints) -> Size {
        let res = cx.resolution();
        let maximum = self.maximum.max(self.minimum);
        let size = constraints.constrain(Size::new(
            res.extent(
                Axis::Horizontal,
                self.size.width.clamp(self.minimum.width, maximum.width),
            ),
            res.extent(
                Axis::Vertical,
                self.size.height.clamp(self.minimum.height, maximum.height),
            ),
        ));
        let grip = Size::new(
            res.extent(Axis::Horizontal, self.grip_size.width)
                .min(size.width),
            res.extent(Axis::Vertical, self.grip_size.height)
                .min(size.height),
        );
        for child in cx.children() {
            let (position, child_size, z_index) = match *cx.item(child) {
                Item::Content => (Point::ZERO, size, 0),
                Item::Right => (
                    Point::new(size.width - grip.width, 0.0),
                    Size::new(grip.width, size.height),
                    1,
                ),
                Item::Bottom => (
                    Point::new(0.0, size.height - grip.height),
                    Size::new(size.width, grip.height),
                    1,
                ),
                Item::Corner => (
                    Point::new(size.width - grip.width, size.height - grip.height),
                    grip,
                    2,
                ),
            };
            cx.layout_child(child, Constraints::tight(child_size));
            cx.set_child_position(child, position);
            cx.set_child_z_index(child, z_index);
        }
        size
    }

    fn override_size(&self, _: &mut Self::Item, _: Option<f32>, _: Option<f32>) -> bool {
        false
    }
}
