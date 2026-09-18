use blit::{Axis, Constraints, Layout as LayoutTrait, LayoutCx, Platform, Point, Size};

#[derive(Clone, Copy)]
pub struct Layout {
    pub axis: Axis,
    pub divider_extent: f32,
    pub extent: f32,
    pub minimum_leading: f32,
    pub minimum_trailing: f32,
}

#[derive(Clone, Copy)]
pub enum Item {
    Leading,
    Divider,
    Trailing,
}

impl<P: Platform> LayoutTrait<P> for Layout {
    type Item = Item;

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, bounds: Constraints) -> Size {
        let cross_axis = self.axis.other();
        let res = cx.resolution();
        let main = self.axis.extent(bounds.max);
        assert!(main.is_finite(), "split needs a finite main axis budget");
        let mut leading = None;
        let mut trailing = None;
        let mut divider = None;
        for child in cx.children() {
            match cx.item(child) {
                Item::Leading => leading = Some(child),
                Item::Trailing => trailing = Some(child),
                Item::Divider => divider = Some(child),
            }
        }
        let leading = leading.expect("missing split leading content");
        let trailing = trailing.expect("missing split trailing content");
        let divider = divider.expect("missing split divider");
        let divider_extent = res
            .extent(self.axis, self.divider_extent)
            .max(0.0)
            .min(main);
        let available = (main - divider_extent).max(0.0);
        let minimum_leading = res.extent(self.axis, self.minimum_leading).max(0.0);
        let minimum_trailing = res.extent(self.axis, self.minimum_trailing).max(0.0);
        let desired = res.extent(self.axis, self.extent).max(0.0);
        let leading_extent = if minimum_leading + minimum_trailing <= available {
            desired.clamp(minimum_leading, available - minimum_trailing)
        } else if minimum_leading + minimum_trailing > 0.0 {
            available * minimum_leading / (minimum_leading + minimum_trailing)
        } else {
            desired.min(available)
        };
        let mut cross = cross_axis.extent(bounds.min);
        for (child, extent, offset) in [
            (leading, leading_extent, 0.0),
            (
                trailing,
                available - leading_extent,
                leading_extent + divider_extent,
            ),
        ] {
            let mut child_bounds = bounds;
            self.axis.set_extent(&mut child_bounds.min, extent);
            self.axis.set_extent(&mut child_bounds.max, extent);
            let size = cx.layout_child(child, child_bounds);
            cross = cross.max(cross_axis.extent(size));
            let mut point = Size::ZERO;
            self.axis.set_extent(&mut point, offset);
            cx.set_child_position(child, Point::new(point.width, point.height));
        }
        let mut size = Size::ZERO;
        self.axis.set_extent(&mut size, divider_extent);
        cross_axis.set_extent(&mut size, cross);
        cx.layout_child(divider, Constraints::tight(size));
        let mut point = Size::ZERO;
        self.axis.set_extent(&mut point, leading_extent);
        cx.set_child_position(divider, Point::new(point.width, point.height));
        self.axis.set_extent(&mut size, main);
        bounds.constrain(size)
    }

    fn override_size(&self, _: &mut Self::Item, _: Option<f32>, _: Option<f32>) -> bool {
        false
    }
}
