use blit::{Axis, Child, Constraints, IntrinsicQuery, IntrinsicSize, LayoutCx, LogicalPoint, LogicalSize, MeasureCx};

use crate::{
    Align, Justify, Padding, Sizing, Unit, flow_constraints, flow_size, flow_sizing, intrinsic_child, intrinsic_cross,
    intrinsic_range, justify_offset, size::axis_sizing, sizing_range,
};

blit::builder! {
    #[const]
    /// lays out children in a row or column
    ///
    /// grow children reserve their content minimum before sharing spare space
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout<U: Unit> {
        new(axis: Axis),
        padding: Padding<U> = Padding::all(U::ZERO),
        gap: U = U::ZERO,
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
    pub struct Item<U: Unit> {
        new(),
        width: Sizing<U> = Sizing::fit(),
        height: Sizing<U> = Sizing::fit(),
        weight: f32 = 1.0,
        basis: Basis = Basis::Minimum,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Basis {
    /// reserves only the item's explicit minimum
    Zero,
    #[default]
    /// reserves the content's intrinsic minimum
    Minimum,
    /// starts at preferred size and shrinks toward the intrinsic minimum
    Preferred,
}

#[derive(Clone, Copy, Default)]
struct Share {
    min: f32,
    size: f32,
    max: f32,
    weight: f32,
}

impl<U: Unit> Item<U> {
    pub const fn fixed(mut self, width: U, height: U) -> Self {
        self.width = Sizing::fixed(width);
        self.height = Sizing::fixed(height);
        self
    }
    pub const fn grow(mut self) -> Self {
        self.width = Sizing::grow();
        self.height = Sizing::grow();
        self
    }
}

impl<C, U: Unit> blit::Layout<C> for Layout<U> {
    type Item = Item<U>;

    fn intrinsic(&self, cx: &mut MeasureCx<'_, C, Self::Item>, query: IntrinsicQuery) -> IntrinsicSize {
        let padding: blit::Sides = self.padding.into();
        let queried_padding = query.axis.extent(padding.size());
        let gap = self.gap.into_float().max(0.0);
        let count = cx.children().count();
        let gaps = gap * count.saturating_sub(1) as f32;
        if query.axis == self.axis {
            let mut result = IntrinsicSize {
                min: queried_padding + gaps,
                preferred: queried_padding + gaps,
            };
            for child in cx.children() {
                let (main, cross) = flow_sizing(self.axis, child.item.width, child.item.height, (None, None));
                let size = intrinsic_child::<_, _, U>(
                    cx,
                    child.id,
                    IntrinsicQuery {
                        axis: query.axis,
                        cross: query
                            .cross
                            .map(|value| (value - self.axis.other().extent(padding.size())).max(0.0)),
                    },
                    main,
                    cross,
                    self.align == Align::Stretch,
                );
                result.min += U::round(size.min);
                result.preferred += U::round(size.preferred);
            }
            return result;
        }

        let mut result = IntrinsicSize {
            min: queried_padding,
            preferred: queried_padding,
        };
        let Some(budget) = query.cross else {
            for child in cx.children() {
                let (cross, main) = flow_sizing(query.axis, child.item.width, child.item.height, (None, None));
                let size = if let Sizing::Fixed(value) = cross {
                    IntrinsicSize {
                        min: value.max(0.0),
                        preferred: value.max(0.0),
                    }
                } else {
                    cx.intrinsic(
                        child.id,
                        IntrinsicQuery {
                            axis: query.axis,
                            cross: intrinsic_cross(main, None),
                        },
                    )
                };
                let size = intrinsic_range(cross, size);
                result.min = result.min.max(U::round(size.min) + queried_padding);
                result.preferred = result.preferred.max(U::round(size.preferred) + queried_padding);
            }
            return result;
        };
        let pool = (U::round(budget) - self.axis.extent(padding.size()) - gaps).max(0.0);
        let mut shares = cx.scratch(count, Share::default());
        let mut minimums = 0.0;
        for (index, child) in cx.children().enumerate() {
            let (main, cross) = flow_sizing(self.axis, child.item.width, child.item.height, (None, None));
            if matches!(main, Sizing::Grow { .. }) {
                let share = self.share(cx, child, main, cross, None);
                minimums += share.min;
                cx.scratch_mut(&mut shares)[index] = share;
            }
        }
        let mut remaining = (pool - minimums).max(0.0);
        let mut percentages = 0.0;
        for (index, child) in cx.children().enumerate() {
            let (main, cross) = flow_sizing(self.axis, child.item.width, child.item.height, (None, None));
            if matches!(main, Sizing::Grow { .. }) {
                continue;
            }
            let available = if matches!(main, Sizing::Percent(_)) {
                pool
            } else if self.overflow {
                f32::INFINITY
            } else {
                remaining
            };
            let range = sizing_range::<U>(main, available);
            let assigned = if matches!(main, Sizing::Fixed(_) | Sizing::Percent(_)) {
                U::distribute(&mut percentages, range.0)
            } else {
                let size = intrinsic_child::<_, _, U>(
                    cx,
                    child.id,
                    IntrinsicQuery {
                        axis: self.axis,
                        cross: None,
                    },
                    main,
                    cross,
                    false,
                );
                U::round(size.preferred).clamp(range.0, range.1)
            };
            cx.scratch_mut(&mut shares)[index] = Share {
                min: assigned,
                size: assigned,
                max: assigned,
                weight: 0.0,
            };
            remaining = (remaining - assigned).max(0.0);
        }
        let capped = cx
            .scratch_mut(&mut shares)
            .iter()
            .filter(|share| share.weight > 0.0 && share.max.is_finite())
            .count();
        let mut caps = cx.scratch(capped, (0.0f32, 0.0f32, 0.0f32));
        distribute(
            cx.scratch_mut(&mut shares),
            cx.scratch_mut(&mut caps),
            remaining + minimums,
        );
        let mut ideal = 0.0;
        for (index, child) in cx.children().enumerate() {
            let cross = axis_sizing(query.axis, child.item.width, child.item.height, (None, None));
            let share = cx.scratch_mut(&mut shares)[index];
            let assigned = if share.weight > 0.0 {
                U::distribute(&mut ideal, share.size)
            } else {
                share.size
            };
            let size = if let Sizing::Fixed(value) = cross {
                IntrinsicSize {
                    min: value.max(0.0),
                    preferred: value.max(0.0),
                }
            } else {
                cx.intrinsic(
                    child.id,
                    IntrinsicQuery {
                        axis: query.axis,
                        cross: Some(assigned),
                    },
                )
            };
            let size = intrinsic_range(cross, size);
            result.min = result.min.max(U::round(size.min) + queried_padding);
            result.preferred = result.preferred.max(U::round(size.preferred) + queried_padding);
        }
        result
    }

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> LogicalSize {
        let padding: blit::Sides = self.padding.into();
        let cross_axis = self.axis.other();
        let gap = self.gap.into_float().max(0.0);
        let main_padding = self.axis.extent(padding.size());
        let cross_padding = cross_axis.extent(padding.size());
        let leading = LogicalSize::new(padding.left, padding.top);
        let main_leading = self.axis.extent(leading);
        let cross_leading = cross_axis.extent(leading);
        let main_max = (U::round(self.axis.extent(bounds.max)) - main_padding).max(0.0);
        let cross_max = (U::round(cross_axis.extent(bounds.max)) - cross_padding).max(0.0);
        let tight_cross = cross_axis.extent(bounds.min) == cross_axis.extent(bounds.max);
        let (count, grows) = cx.children().fold((0usize, 0usize), |(count, grows), child| {
            let main = axis_sizing(
                self.axis,
                child.item.width,
                child.item.height,
                cx.size_overrides(child.id),
            );
            (count + 1, grows + usize::from(matches!(main, Sizing::Grow { .. })))
        });
        let mut shares = cx.scratch(count, Share::default());
        let mut minimums = 0.0;
        for (index, child) in cx.children().enumerate() {
            let overrides = cx.size_overrides(child.id);
            let main = axis_sizing(self.axis, child.item.width, child.item.height, overrides);
            if matches!(main, Sizing::Grow { .. }) {
                let cross = axis_sizing(cross_axis, child.item.width, child.item.height, overrides);
                let share = self.share(cx, child, main, cross, tight_cross.then_some(cross_max));
                minimums += share.min;
                cx.scratch_mut(&mut shares)[index] = share;
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
        let cross_bounds = |sizing: Sizing<f32>| {
            let sizing = sizing.map(U::round);
            let range = sizing_range::<U>(sizing, cross_max);
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
        let mut percentages = 0.0;
        for (index, child) in cx.children().enumerate() {
            let item = child.item;
            let (sizing, cross_sizing) = flow_sizing(self.axis, item.width, item.height, cx.size_overrides(child.id));
            let sizing = sizing.map(U::round);
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
            let mut main_bounds = sizing_range::<U>(sizing, budget);
            if matches!(sizing, Sizing::Percent(_)) {
                let main = U::distribute(&mut percentages, main_bounds.0);
                main_bounds = (main, main);
            }
            let child_bounds = flow_constraints::<U>(self.axis, main_bounds, cross_bounds(cross_sizing));
            let size = cx.layout_child(child.id, child_bounds);
            let main = U::round(self.axis.extent(size));
            cx.scratch_mut(&mut shares)[index] = Share {
                min: main,
                size: main,
                max: main,
                weight: 0.0,
            };
            used += main;
            remaining = (remaining - main).max(0.0);
            cross = cross.max(U::round(cross_axis.extent(size)));
        }
        if grows != 0 {
            let capped = cx
                .scratch_mut(&mut shares)
                .iter()
                .filter(|share| share.weight > 0.0 && share.max.is_finite())
                .count();
            let mut caps = cx.scratch(capped, (0.0f32, 0.0f32, 0.0f32));
            distribute(
                cx.scratch_mut(&mut shares),
                cx.scratch_mut(&mut caps),
                remaining + minimums,
            );
            let mut ideal = 0.0;
            for (index, child) in cx.children().enumerate() {
                let share = cx.scratch_mut(&mut shares)[index];
                if share.weight == 0.0 {
                    continue;
                }
                let item = child.item;
                let cross_sizing = axis_sizing(cross_axis, item.width, item.height, cx.size_overrides(child.id));
                let main = U::distribute(&mut ideal, share.size);
                let child_bounds = flow_constraints::<U>(self.axis, (main, main), cross_bounds(cross_sizing));
                let size = cx.layout_child(child.id, child_bounds);
                used += U::round(self.axis.extent(size));
                cross = cross.max(U::round(cross_axis.extent(size)));
            }
        }
        let size = bounds.constrain(flow_size(used + gaps + main_padding, cross + cross_padding, self.axis));
        let available_main = (U::round(self.axis.extent(size)) - main_padding).max(0.0);
        let available_cross = (U::round(cross_axis.extent(size)) - cross_padding).max(0.0);
        let (offset, extra_gap) = justify_offset(self.justify, (available_main - used - gaps).max(0.0), count);
        let mut cursor = main_leading + offset;
        for child in cx.children() {
            let child_size = cx.size(child.id);
            let child_cross = U::round(cross_axis.extent(child_size));
            let offset = match self.align {
                Align::Start | Align::Stretch => 0.0,
                Align::Center => (available_cross - child_cross).max(0.0) / 2.0,
                Align::End => (available_cross - child_cross).max(0.0),
            };
            let pos = flow_size(U::round(cursor), U::round(cross_leading + offset), self.axis);
            cx.set_position(child.id, LogicalPoint::new(pos.width, pos.height));
            cursor += U::round(self.axis.extent(child_size)) + gap + extra_gap;
        }
        size
    }
}

impl<U: Unit> Layout<U> {
    #[inline(always)]
    fn share<C>(
        &self,
        cx: &mut MeasureCx<'_, C, Item<U>>,
        child: Child<'_, Item<U>>,
        main: Sizing<f32>,
        cross: Sizing<f32>,
        cross_extent: Option<f32>,
    ) -> Share {
        let Sizing::Grow { min, max } = main.map(U::round) else {
            unreachable!()
        };
        assert!(
            child.item.weight.is_finite() && child.item.weight > 0.0,
            "flex weight must be finite and positive"
        );
        let natural = if child.item.basis == Basis::Zero {
            IntrinsicSize::default()
        } else {
            intrinsic_child::<_, _, U>(
                cx,
                child.id,
                IntrinsicQuery {
                    axis: self.axis,
                    cross: cross_extent,
                },
                main,
                cross,
                self.align == Align::Stretch,
            )
        };
        let max = max.unwrap_or(f32::INFINITY).max(min).max(0.0);
        let min = U::round(natural.min).max(min).max(0.0).min(max);
        assert!(min.is_finite(), "flex minimum must be finite");
        let size = if child.item.basis == Basis::Preferred {
            U::round(natural.preferred).clamp(min, max)
        } else {
            min
        };
        Share {
            min,
            size,
            max,
            weight: child.item.weight,
        }
    }
}

fn distribute(shares: &mut [Share], caps: &mut [(f32, f32, f32)], budget: f32) {
    let (minimum, demand) = shares
        .iter()
        .filter(|share| share.weight > 0.0)
        .fold((0.0, 0.0), |(minimum, demand), share| {
            (minimum + share.min, demand + (share.size - share.min))
        });
    let fraction = if demand > 0.0 {
        ((budget - minimum).max(0.0) / demand).min(1.0)
    } else {
        0.0
    };
    let mut remaining = budget.max(minimum);
    let mut weights = 0.0;
    let mut capped = 0;
    for share in shares.iter_mut().filter(|share| share.weight > 0.0) {
        share.size = share.min + (share.size - share.min) * fraction;
        remaining -= share.size;
        let capacity = share.max - share.size;
        if capacity > 0.0 {
            weights += share.weight;
            if capacity.is_finite() {
                caps[capped] = (capacity / share.weight, capacity, share.weight);
                capped += 1;
            }
        }
    }
    remaining = remaining.max(0.0);
    let caps = &mut caps[..capped];
    caps.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    for &(limit, capacity, weight) in caps.iter() {
        if weights == 0.0 || limit >= remaining / weights {
            break;
        }
        remaining = (remaining - capacity).max(0.0);
        weights = (weights - weight).max(0.0);
    }
    let unit = if weights > 0.0 {
        remaining / weights
    } else {
        f32::INFINITY
    };
    for share in shares.iter_mut().filter(|share| share.weight > 0.0) {
        share.size += (unit * share.weight).min((share.max - share.size).max(0.0));
    }
}
