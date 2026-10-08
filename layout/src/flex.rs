use blit::{Axis, Constraints, Context, LayoutCx, Point, Scalar, Sides, Size, Sizing};

use super::{Align, Justify, allocated_range, flow_constraints, flow_size, justify_offset, sizing_range};

blit::builder! {
    /// lays out children in a row or column
    ///
    /// fixed and fit children are sized first, then grow children share what is left
    /// loose cross axis grow and stretch wait for content sizes
    /// without content sizes their cross axis budget is zero
    /// unbounded main axis grow uses natural content sizes
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout<T: Scalar> {
        new(axis: Axis),
        padding: Sides<T> = Sides::all(T::ZERO),
        gap: T = T::ZERO,
        align: Align = Align::Start,
        justify: Justify = Justify::Start,
        overflow: bool = false,
    }
}

blit::builder! {
    /// sizing overrides and growth weight for a flex child
    ///
    /// unspecified axes use the child's declared sizing
    /// weight only affects how grow children share leftover space
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Item<T: Scalar> {
        new(),
        #[into]
        width: Option<Sizing<T>> = None,
        #[into]
        height: Option<Sizing<T>> = None,
        weight: f32 = 1.0,
    }
}

impl<T: Scalar> Item<T> {
    pub fn fixed(mut self, width: T, height: T) -> Self {
        self.width = Some(Sizing::fixed(width));
        self.height = Some(Sizing::fixed(height));
        self
    }
    pub fn grow(mut self) -> Self {
        self.width = Some(Sizing::grow());
        self.height = Some(Sizing::grow());
        self
    }
    pub fn sizing(&self, axis: Axis) -> Option<Sizing<T>> {
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
            if let Sizing::Grow { min, max } = resolve_sizing(cx, child, self.axis) {
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

        let padding = self.padding;
        let cross_axis = self.axis.other();
        let gap = self.gap.max(T::ZERO);
        let main_padding = self.axis.extent(padding.size());
        let cross_padding = cross_axis.extent(padding.size());
        let inner_bounds = bounds.shrink(padding.size());
        let main_max = self.axis.extent(inner_bounds.max);
        let cross_max = cross_axis.extent(inner_bounds.max);
        let mut caps = cx.scratch(
            if main_max.is_finite() { cap_count } else { 0 },
            (0.0_f32, T::ZERO, 0.0),
        );
        if count == 0 {
            return bounds.constrain(padding.size());
        }
        let gaps = gap.repeat(count.saturating_sub(1));
        let pool = if main_max.is_finite() {
            (main_max - gaps).max(T::ZERO)
        } else {
            T::UNBOUNDED
        };
        let mut space = Space {
            pool,
            remaining: if pool.is_finite() {
                (pool - minimums).max(T::ZERO)
            } else {
                T::UNBOUNDED
            },
            used: T::ZERO,
            cross: T::ZERO,
            cross_max,
            dynamic_cross: cross_axis.extent(inner_bounds.min) != cross_max || !cross_max.is_finite(),
        };
        let mut cap_index = 0;
        let mut cross_fill = false;
        let mut deferred = false;
        let mut percentages = 0.0;
        for child in cx.children() {
            let item = cx.item(child);
            let sizing = resolve_sizing(cx, child, self.axis);
            let cross = resolve_sizing(cx, child, cross_axis);
            let main_bounds = self.main_bounds(sizing, &space, &mut percentages);
            if let Sizing::Grow { min, max } = sizing
                && main_max.is_finite()
            {
                let min = min.max(T::ZERO);
                if max > min && max.is_finite() {
                    let capacity = max - min;
                    caps[cap_index] = (capacity.to_f32() / item.weight, capacity, item.weight);
                    cap_index += 1;
                }
                continue;
            }
            if space.dynamic_cross
                && matches!(
                    (cross, self.align),
                    (Sizing::Grow { .. }, _) | (Sizing::Fit { .. }, Align::Stretch)
                )
            {
                if let Sizing::Fixed(main) = sizing {
                    if space.remaining.is_finite() {
                        space.remaining = (space.remaining - main.max(T::ZERO)).max(T::ZERO);
                    }
                    deferred = true;
                } else {
                    cross_fill = true;
                }
                continue;
            }
            self.layout_child(cx, child, main_bounds, cross, &mut space);
        }
        if cross_fill {
            let mut percentages = 0.0;
            for child in cx.children() {
                let sizing = resolve_sizing(cx, child, self.axis);
                let cross = resolve_sizing(cx, child, cross_axis);
                let main_bounds = self.main_bounds(sizing, &space, &mut percentages);
                if matches!(sizing, Sizing::Fixed(_))
                    || main_max.is_finite() && matches!(sizing, Sizing::Grow { .. })
                    || !matches!(
                        (cross, self.align),
                        (Sizing::Grow { .. }, _) | (Sizing::Fit { .. }, Align::Stretch)
                    )
                {
                    continue;
                }
                self.layout_child(cx, child, main_bounds, cross, &mut space);
            }
        }
        if grows != 0 && main_max.is_finite() {
            // saturate caps in threshold order without revisiting a child layout
            caps.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
            let mut unit = if weights > 0.0 {
                space.remaining.to_f32() / weights
            } else {
                0.0
            };
            for &(limit, capacity, weight) in caps.iter() {
                if limit >= unit {
                    break;
                }
                space.remaining = (space.remaining - capacity).max(T::ZERO);
                weights = (weights - weight).max(0.0);
                unit = if weights > 0.0 {
                    space.remaining.to_f32() / weights
                } else {
                    f32::INFINITY
                };
            }
            let mut allocation = 0.0;
            for child in cx.children() {
                let item = cx.item(child);
                let Sizing::Grow { min, max } = resolve_sizing(cx, child, self.axis) else {
                    continue;
                };
                let min = min.max(T::ZERO);
                let capacity = if max == T::UNBOUNDED {
                    f32::INFINITY
                } else {
                    (max.max(min) - min).to_f32()
                };
                let main = min + T::allocate(&mut allocation, (unit * item.weight).min(capacity));
                self.layout_child(
                    cx,
                    child,
                    (main, main),
                    resolve_sizing(cx, child, cross_axis),
                    &mut space,
                );
            }
        }
        if deferred {
            for child in cx.children() {
                let Sizing::Fixed(main) = resolve_sizing(cx, child, self.axis) else {
                    continue;
                };
                let cross = resolve_sizing(cx, child, cross_axis);
                if !matches!(
                    (cross, self.align),
                    (Sizing::Grow { .. }, _) | (Sizing::Fit { .. }, Align::Stretch)
                ) {
                    continue;
                }
                let main = main.max(T::ZERO);
                self.layout_child(cx, child, (main, main), cross, &mut space);
            }
        }
        let size = bounds.constrain(flow_size(
            space.used + gaps + main_padding,
            space.cross + cross_padding,
            self.axis,
        ));
        let leading = Size::new(padding.left, padding.top);
        let main_leading = self.axis.extent(leading);
        let cross_leading = cross_axis.extent(leading);
        let available_main = (self.axis.extent(size) - main_padding).max(T::ZERO);
        let available_cross = (cross_axis.extent(size) - cross_padding).max(T::ZERO);
        let (offset, mut spacing, extra_gap) =
            justify_offset(self.justify, (available_main - space.used - gaps).max(T::ZERO), count);
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

impl<T: Scalar> Layout<T> {
    fn layout_child<C: Context<Scalar = T>>(
        &self,
        cx: &mut LayoutCx<'_, C, Item<T>>,
        child: blit::NodeId,
        main: (T, T),
        cross: Sizing<T>,
        space: &mut Space<T>,
    ) {
        let cross = match (cross, self.align) {
            (Sizing::Grow { .. }, _) | (Sizing::Fit { .. }, Align::Stretch) => {
                let extent = cross.clamp(if space.dynamic_cross {
                    space.cross
                } else {
                    space.cross_max
                });
                (extent, extent)
            }
            _ => sizing_range(cross, space.cross_max),
        };
        let size = cx.layout_child(child, flow_constraints(self.axis, main, cross));
        let main = self.axis.extent(size);
        space.used += main;
        if space.remaining.is_finite() {
            space.remaining = (space.remaining - main).max(T::ZERO);
        }
        space.cross = space.cross.max(self.axis.other().extent(size));
    }

    fn main_bounds(&self, sizing: Sizing<T>, space: &Space<T>, percentages: &mut f32) -> (T, T) {
        let budget = match sizing {
            Sizing::Percent(_) => space.pool,
            _ if self.overflow => T::UNBOUNDED,
            _ => space.remaining,
        };
        allocated_range(sizing, budget, percentages)
    }
}

struct Space<T> {
    pool: T,
    remaining: T,
    used: T,
    cross: T,
    cross_max: T,
    dynamic_cross: bool,
}

fn resolve_sizing<C: Context>(
    cx: &LayoutCx<'_, C, Item<C::Scalar>>,
    child: blit::NodeId,
    axis: Axis,
) -> Sizing<C::Scalar> {
    let sizing = cx
        .item(child)
        .sizing(axis)
        .unwrap_or_else(|| cx.child_sizing(child, axis));
    super::resolve_sizing(cx, child, axis, sizing)
}
