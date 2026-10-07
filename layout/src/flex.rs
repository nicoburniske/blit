use blit::{Axis, Constraints, Context, LayoutCx, Point, Scalar, Sides, Size};

use super::{
    Align, Justify, Sizing, allocated_range, flow_constraints, flow_size, justify_offset, resolve_sizing, sizing_range,
};

blit::builder! {
    /// lays out children in a row or column
    ///
    /// fixed and fit children are sized first, then grow children share what is left
    /// grow does not account for the preferred sizes of nested content
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout<T: Scalar> {
        new(axis: Axis),
        padding: Sides<T> = Sides::all(T::ZERO),
        gap: T = T::ZERO,
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
    pub struct Item<T: Scalar> {
        new(),
        width: Sizing<T> = Sizing::fit(),
        height: Sizing<T> = Sizing::fit(),
        weight: f32 = 1.0,
    }
}

impl<T: Scalar> Item<T> {
    pub fn fixed(mut self, width: T, height: T) -> Self {
        self.width = Sizing::fixed(width);
        self.height = Sizing::fixed(height);
        self
    }
    pub fn grow(mut self) -> Self {
        self.width = Sizing::grow();
        self.height = Sizing::grow();
        self
    }
    pub fn sizing(&self, axis: Axis) -> Sizing<T> {
        match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => self.height,
        }
    }
}

pub fn layout<T: Scalar>(axis: Axis) -> Layout<T> {
    Layout::new(axis)
}

pub fn row<T: Scalar>() -> Layout<T> {
    layout(Axis::Horizontal)
}

pub fn column<T: Scalar>() -> Layout<T> {
    layout(Axis::Vertical)
}

pub fn item<T: Scalar>() -> Item<T> {
    Item::new()
}

impl<C: Context<Scalar = T>, T: Scalar> blit::Layout<C> for Layout<T> {
    type Item = Item<T>;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints<T>) -> Size<T> {
        let mut count = 0usize;
        let mut grows = 0usize;
        let mut minimums = T::ZERO;
        let mut weights = 0.0;
        // only capped shares need scratch storage and sorting
        let mut cap_count = 0;
        for child in cx.children() {
            count += 1;
            let item = cx.item(child);
            if let Sizing::Grow { min, max } = resolve_sizing(cx, child, self.axis, item.sizing(self.axis)) {
                assert!(
                    item.weight.is_finite() && item.weight > 0.0,
                    "flex weight must be finite and positive"
                );
                let min = min.max(T::ZERO);
                assert!(min.is_finite(), "flex minimum must be finite");
                grows += 1;
                minimums += min;
                if max > min {
                    weights += item.weight;
                    if max.is_finite() {
                        cap_count += 1;
                    }
                }
            }
        }

        let mut caps = cx.scratch(cap_count, (0.0, T::ZERO, 0.0));
        let padding = self.padding;
        let cross_axis = self.axis.other();
        let gap = self.gap.max(T::ZERO);
        let main_padding = self.axis.extent(padding.size());
        let cross_padding = cross_axis.extent(padding.size());
        let maximum_size = bounds.shrink(padding.size()).max;
        let main_max = self.axis.extent(maximum_size);
        let cross_max = cross_axis.extent(maximum_size);
        let tight_cross = cross_axis.extent(bounds.min) == cross_axis.extent(bounds.max);
        if count == 0 {
            return bounds.constrain(padding.size());
        }
        let mut cap_index = 0;
        assert!(
            grows == 0 || main_max.is_finite(),
            "main axis grow requires a finite budget"
        );
        let gaps = gap.repeat(count.saturating_sub(1));
        let pool = if main_max.is_finite() {
            (main_max - gaps).max(T::ZERO)
        } else {
            T::UNBOUNDED
        };
        let mut remaining = if pool.is_finite() {
            (pool - minimums).max(T::ZERO)
        } else {
            T::UNBOUNDED
        };
        let mut used = T::ZERO;
        let mut cross = T::ZERO;
        let cross_bounds = |sizing| match (sizing, self.align) {
            (Sizing::Grow { .. }, _) | (Sizing::Fit { .. }, Align::Stretch) if tight_cross => {
                let extent = sizing.clamp(cross_max);
                assert!(extent.is_finite(), "cross axis grow requires a finite budget");
                (extent, extent)
            }
            _ => sizing_range(sizing, cross_max),
        };
        let mut percentages = 0.0;
        if grows < count || !caps.is_empty() {
            for child in cx.children() {
                let item = cx.item(child);
                let sizing = resolve_sizing(cx, child, self.axis, item.sizing(self.axis));
                if let Sizing::Grow { min, max } = sizing {
                    let min = min.max(T::ZERO);
                    if max > min && max.is_finite() {
                        let capacity = max - min;
                        caps[cap_index] = (capacity.to_f32() / item.weight, capacity, item.weight);
                        cap_index += 1;
                    }
                    continue;
                }
                let budget = match sizing {
                    Sizing::Percent(_) => pool,
                    _ if self.overflow => T::UNBOUNDED,
                    _ => remaining,
                };
                let child_bounds = flow_constraints(
                    self.axis,
                    allocated_range(sizing, budget, &mut percentages),
                    cross_bounds(resolve_sizing(cx, child, cross_axis, item.sizing(cross_axis))),
                );
                let size = cx.layout_child(child, child_bounds);
                let main = self.axis.extent(size);
                used += main;
                remaining = if remaining.is_finite() {
                    (remaining - main).max(T::ZERO)
                } else {
                    T::UNBOUNDED
                };
                cross = cross.max(cross_axis.extent(size));
            }
        }
        if grows != 0 {
            // saturate caps in threshold order without revisiting a child layout
            caps.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
            let mut unit = if weights > 0.0 {
                remaining.to_f32() / weights
            } else {
                0.0
            };
            for &(limit, capacity, weight) in caps.iter() {
                if limit >= unit {
                    break;
                }
                remaining = (remaining - capacity).max(T::ZERO);
                weights = (weights - weight).max(0.0);
                unit = if weights > 0.0 {
                    remaining.to_f32() / weights
                } else {
                    f32::INFINITY
                };
            }
            let mut allocation = 0.0;
            for child in cx.children() {
                let item = cx.item(child);
                let sizing = resolve_sizing(cx, child, self.axis, item.sizing(self.axis));
                let Sizing::Grow { min, max } = sizing else {
                    continue;
                };
                let min = min.max(T::ZERO);
                let capacity = if max == T::UNBOUNDED {
                    f32::INFINITY
                } else {
                    (max.max(min) - min).to_f32()
                };
                let main = min + T::allocate(&mut allocation, (unit * item.weight).min(capacity));
                let child_bounds = flow_constraints(
                    self.axis,
                    (main, main),
                    cross_bounds(resolve_sizing(cx, child, cross_axis, item.sizing(cross_axis))),
                );
                let size = cx.layout_child(child, child_bounds);
                used += self.axis.extent(size);
                cross = cross.max(cross_axis.extent(size));
            }
        }
        let size = bounds.constrain(flow_size(used + gaps + main_padding, cross + cross_padding, self.axis));
        let leading = Size::new(padding.left, padding.top);
        let main_leading = self.axis.extent(leading);
        let cross_leading = cross_axis.extent(leading);
        let available_main = (self.axis.extent(size) - main_padding).max(T::ZERO);
        let available_cross = (cross_axis.extent(size) - cross_padding).max(T::ZERO);
        let (offset, mut spacing, extra_gap) =
            justify_offset(self.justify, (available_main - used - gaps).max(T::ZERO), count);
        let mut cursor = main_leading + offset;
        for child in cx.children() {
            let child_size = cx.size(child);
            let child_cross = cross_axis.extent(child_size);
            let offset = match self.align {
                Align::Start | Align::Stretch => T::ZERO,
                Align::Center => T::ZERO.lerp((available_cross - child_cross).max(T::ZERO), 0.5),
                Align::End => (available_cross - child_cross).max(T::ZERO),
            };
            let pos = flow_size(cursor + T::from_f32(spacing), cross_leading + offset, self.axis);
            cx.set_child_position(child, Point::new(pos.width, pos.height));
            cursor += self.axis.extent(child_size) + gap;
            spacing += extra_gap;
        }
        size
    }
}
