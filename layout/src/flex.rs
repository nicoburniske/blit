use blit::{Axis, Constraints, LayoutCx, LogicalPoint, LogicalSize};

use crate::{
    Align, Justify, Length, Padding, Sizing, distribute, flow_constraints, flow_size, flow_sizing, justify_offset,
    round, round_sizing, sizing_range,
};

blit::builder! {
    #[const]
    /// lays out children in a row or column
    ///
    /// fixed and fit children are sized first, then grow children share what is left
    /// grow does not account for the preferred sizes of nested content
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout {
        new(axis: Axis),
        padding: Padding = Padding::all(0 as Length),
        gap: Length = 0 as Length,
        align: Align = Align::Stretch,
        justify: Justify = Justify::Start,
        overflow: bool = false,
    }
}

blit::builder! {
    #[const]
    /// sizing and growth weight for a flex child
    ///
    /// weight only affects how grow children share leftover space
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Item {
        new(),
        width: crate::Sizing = crate::Sizing::fit(),
        height: crate::Sizing = crate::Sizing::fit(),
        weight: f32 = 1.0,
    }
}

impl Item {
    pub const fn fixed(mut self, width: Length, height: Length) -> Self {
        self.width = crate::Sizing::fixed(width);
        self.height = crate::Sizing::fixed(height);
        self
    }
    pub const fn grow(mut self) -> Self {
        self.width = crate::Sizing::grow();
        self.height = crate::Sizing::grow();
        self
    }
}

pub const fn layout(axis: Axis) -> Layout {
    Layout::new(axis)
}

pub const fn row() -> Layout {
    layout(Axis::Horizontal)
}

pub const fn column() -> Layout {
    layout(Axis::Vertical)
}

pub const fn item() -> Item {
    Item::new()
}

impl<C> blit::Layout<C> for Layout {
    type Item = Item;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> LogicalSize {
        let padding: blit::Sides = self.padding.into();
        let cross_axis = self.axis.other();
        let gap = (self.gap as f32).max(0.0);
        let main_padding = self.axis.extent(padding.size());
        let cross_padding = cross_axis.extent(padding.size());
        let leading = LogicalSize::new(padding.left, padding.top);
        let main_leading = self.axis.extent(leading);
        let cross_leading = cross_axis.extent(leading);
        let main_max = (round(self.axis.extent(bounds.max)) - main_padding).max(0.0);
        let cross_max = (round(cross_axis.extent(bounds.max)) - cross_padding).max(0.0);
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
            let (width, height) = cx.size_overrides(child);
            let sizing = match self.axis {
                Axis::Horizontal => item.width.with_override(width),
                Axis::Vertical => item.height.with_override(height),
            };
            if let Sizing::Grow { min, max } = round_sizing(sizing) {
                assert!(
                    item.weight.is_finite() && item.weight > 0.0,
                    "flex weight must be finite and positive"
                );
                let min = min.max(0.0);
                let capacity = (max.unwrap_or(f32::INFINITY).max(min) - min).max(0.0);
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
            let sizing = round_sizing(sizing);
            let range = sizing_range(sizing, cross_max);
            if tight_cross
                && (matches!(sizing, Sizing::Grow { .. })
                    || self.align == Align::Stretch && matches!(sizing, Sizing::Fit { .. }))
            {
                let extent = sizing.clamp(cross_max);
                assert!(extent.is_finite(), "cross axis grow requires a finite budget");
                (extent, extent)
            } else {
                range
            }
        };
        for child in cx.children() {
            let item = cx.item(child);
            let (main_sizing, cross_sizing) = flow_sizing(self.axis, item.width, item.height, cx.size_overrides(child));
            let sizing = round_sizing(main_sizing);
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
            let child_bounds = flow_constraints(self.axis, sizing_range(sizing, budget), cross_bounds(cross_sizing));
            let size = cx.layout_child(child, child_bounds);
            let main = round(self.axis.extent(size));
            used += main;
            remaining = (remaining - main).max(0.0);
            cross = cross.max(round(cross_axis.extent(size)));
        }
        if grows != 0 {
            // saturate caps in threshold order without revisiting a child layout
            caps.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
            let mut unit = if weights > 0.0 { remaining / weights } else { 0.0 };
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
            let mut ideal = 0.0;
            for child in cx.children() {
                let item = cx.item(child);
                let (main_sizing, cross_sizing) =
                    flow_sizing(self.axis, item.width, item.height, cx.size_overrides(child));
                let sizing = round_sizing(main_sizing);
                let Sizing::Grow { min, max } = sizing else {
                    continue;
                };
                let min = min.max(0.0);
                let capacity = (max.unwrap_or(f32::INFINITY).max(min) - min).max(0.0);
                let share = min + (unit * item.weight).min(capacity);
                let main = distribute(&mut ideal, share);
                let child_bounds = flow_constraints(self.axis, (main, main), cross_bounds(cross_sizing));
                let size = cx.layout_child(child, child_bounds);
                used += round(self.axis.extent(size));
                cross = cross.max(round(cross_axis.extent(size)));
            }
        }
        let size = bounds.constrain(flow_size(used + gaps + main_padding, cross + cross_padding, self.axis));
        let available_main = (round(self.axis.extent(size)) - main_padding).max(0.0);
        let available_cross = (round(cross_axis.extent(size)) - cross_padding).max(0.0);
        let (offset, extra_gap) = justify_offset(self.justify, (available_main - used - gaps).max(0.0), count);
        let mut cursor = main_leading + offset;
        for child in cx.children() {
            let child_size = cx.size(child);
            let child_cross = round(cross_axis.extent(child_size));
            let offset = match self.align {
                Align::Start | Align::Stretch => 0.0,
                Align::Center => (available_cross - child_cross).max(0.0) / 2.0,
                Align::End => (available_cross - child_cross).max(0.0),
            };
            let pos = flow_size(round(cursor), round(cross_leading + offset), self.axis);
            cx.set_position(child, LogicalPoint::new(pos.width, pos.height));
            cursor += round(self.axis.extent(child_size)) + gap + extra_gap;
        }
        size
    }
}
