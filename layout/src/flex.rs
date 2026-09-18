use blit::{Axis, Constraints, LayoutCx, Platform, Point, Sides, Size, Sizing};

use super::{
    Align, Justify, flow_constraints, flow_size, justify_offset, override_sizing, sizing_range,
};

blit::builder! {
    /// lays out children in a row or column
    ///
    /// fixed and fit children are sized first, then grow children share what is left
    /// grow does not account for the preferred sizes of nested content
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout {
        new(axis: Axis),
        padding: Sides = Sides::all(0.0),
        gap: f32 = 0.0,
        align: Align = Align::Stretch,
        justify: Justify = Justify::Start,
        overflow: bool = false,
    }
}

blit::builder! {
    /// sizing and growth weight for a flex child
    ///
    /// weight only affects how grow children share leftover space
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Item {
        new(),
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
        weight: f32 = 1.0,
    }
}

impl Item {
    pub fn fixed(mut self, width: f32, height: f32) -> Self {
        self.width = Sizing::fixed(width);
        self.height = Sizing::fixed(height);
        self
    }
    pub fn grow(mut self) -> Self {
        self.width = Sizing::grow();
        self.height = Sizing::grow();
        self
    }
    pub fn sizing(&self, axis: Axis) -> Sizing {
        match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => self.height,
        }
    }
}

pub fn layout(axis: Axis) -> Layout {
    Layout::new(axis)
}
pub fn row() -> Layout {
    layout(Axis::Horizontal)
}
pub fn column() -> Layout {
    layout(Axis::Vertical)
}
pub fn item() -> Item {
    Item::new()
}

impl<P: Platform> blit::Layout<P> for Layout {
    type Item = Item;

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, bounds: Constraints) -> Size {
        let res = cx.resolution();
        let padding = res.sides(self.padding);
        let cross_axis = self.axis.other();
        let gap = res.extent(self.axis, self.gap).max(0.0);
        let main_padding = self.axis.extent(padding.size());
        let cross_padding = cross_axis.extent(padding.size());
        let leading = Size::new(padding.left, padding.top);
        let main_leading = self.axis.extent(leading);
        let cross_leading = cross_axis.extent(leading);
        let main_max = (self.axis.extent(bounds.max) - main_padding).max(0.0);
        let cross_max = (cross_axis.extent(bounds.max) - cross_padding).max(0.0);
        let tight_cross = cross_axis.extent(bounds.min) == cross_axis.extent(bounds.max);
        let mut count = 0usize;
        let mut grows = 0usize;
        let mut minimums = 0.0;
        let mut weights = 0.0;
        // only capped shares need scratch storage and sorting
        let mut caps = Vec::new();
        for child in cx.children() {
            count += 1;
            let item = cx.item(child);
            if let Sizing::Grow { min, max } = res.sizing(self.axis, item.sizing(self.axis)) {
                assert!(
                    item.weight.is_finite() && item.weight > 0.0,
                    "flex weight must be finite and positive"
                );
                let min = min.max(0.0);
                let capacity = (max.max(min) - min).max(0.0);
                assert!(min.is_finite(), "flex minimum must be finite");
                grows += 1;
                minimums += min;
                if capacity > 0.0 {
                    weights += item.weight;
                    if capacity.is_finite() {
                        caps.push((capacity / item.weight, capacity, item.weight));
                    }
                }
            }
        }
        if count == 0 {
            return bounds.constrain(padding.size());
        }
        assert!(
            grows == 0 || main_max.is_finite(),
            "main axis grow requires a finite budget"
        );
        let gaps = gap * count.saturating_sub(1) as f32;
        let pool = (main_max - gaps).max(0.0);
        let mut remaining = (pool - minimums).max(0.0);
        let mut used = 0.0;
        let mut cross: f32 = 0.0;
        let cross_bounds = |sizing| {
            let range = sizing_range(sizing, cross_max);
            if tight_cross
                && (matches!(sizing, Sizing::Grow { .. })
                    || self.align == Align::Stretch && matches!(sizing, Sizing::Fit { .. }))
            {
                let extent = sizing.clamp(cross_max);
                assert!(
                    extent.is_finite(),
                    "cross axis grow requires a finite budget"
                );
                (extent, extent)
            } else {
                range
            }
        };
        for child in cx.children() {
            let item = cx.item(child);
            let sizing = res.sizing(self.axis, item.sizing(self.axis));
            if matches!(sizing, Sizing::Grow { .. }) {
                continue;
            }
            let budget = if matches!(sizing, Sizing::Percent(_)) {
                pool
            } else if self.overflow {
                f32::INFINITY
            } else {
                remaining
            };
            let child_bounds = flow_constraints(
                self.axis,
                sizing_range(sizing, budget),
                cross_bounds(res.sizing(cross_axis, item.sizing(cross_axis))),
            );
            let size = cx.layout_child(child, child_bounds);
            let main = self.axis.extent(size);
            used += main;
            remaining = (remaining - main).max(0.0);
            cross = cross.max(cross_axis.extent(size));
        }
        if grows != 0 {
            // saturate caps in threshold order without revisiting a child layout
            caps.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
            let mut unit = if weights > 0.0 {
                remaining / weights
            } else {
                0.0
            };
            for (limit, capacity, weight) in caps {
                if limit >= unit {
                    break;
                }
                remaining = (remaining - capacity).max(0.0);
                weights = (weights - weight).max(0.0);
                unit = if weights > 0.0 {
                    remaining / weights
                } else {
                    f32::INFINITY
                };
            }
            for child in cx.children() {
                let item = cx.item(child);
                let sizing = res.sizing(self.axis, item.sizing(self.axis));
                let Sizing::Grow { min, max } = sizing else {
                    continue;
                };
                let min = min.max(0.0);
                let capacity = (max.max(min) - min).max(0.0);
                let main = min + (unit * item.weight).min(capacity);
                let child_bounds = flow_constraints(
                    self.axis,
                    (main, main),
                    cross_bounds(res.sizing(cross_axis, item.sizing(cross_axis))),
                );
                let size = cx.layout_child(child, child_bounds);
                used += self.axis.extent(size);
                cross = cross.max(cross_axis.extent(size));
            }
        }
        let size = bounds.constrain(flow_size(
            used + gaps + main_padding,
            cross + cross_padding,
            self.axis,
        ));
        let available_main = (self.axis.extent(size) - main_padding).max(0.0);
        let available_cross = (cross_axis.extent(size) - cross_padding).max(0.0);
        let (offset, extra_gap) =
            justify_offset(self.justify, (available_main - used - gaps).max(0.0), count);
        let mut cursor = main_leading + offset;
        for child in cx.children() {
            let child_size = cx.child_size(child);
            let child_cross = cross_axis.extent(child_size);
            let offset = match self.align {
                Align::Start | Align::Stretch => 0.0,
                Align::Center => (available_cross - child_cross).max(0.0) / 2.0,
                Align::End => (available_cross - child_cross).max(0.0),
            };
            let pos = flow_size(cursor, cross_leading + offset, self.axis);
            cx.set_child_position(child, Point::new(pos.width, pos.height));
            cursor += self.axis.extent(child_size) + gap + extra_gap;
        }
        size
    }

    fn override_size(&self, item: &mut Item, width: Option<f32>, height: Option<f32>) -> bool {
        override_sizing(&mut item.width, &mut item.height, width, height)
    }
}
