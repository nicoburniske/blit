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
            if let Sizing::Grow { min, max } = self.resolve(cx, child, item, self.axis) {
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
        };
        let grow = grows != 0 && main_max.is_finite();
        let fixed_cross = cross_axis.extent(inner_bounds.min) == cross_max && cross_max.is_finite();
        if fixed_cross {
            // the cross extent is known, so only main axis growth needs to wait
            let mut cap_index = 0;
            let mut percentages = 0.0;
            for child in cx.children() {
                let item = cx.item(child);
                let sizing = self.resolve(cx, child, item, self.axis);
                let main = self.main_bounds(sizing, &space, &mut percentages);
                match sizing {
                    Sizing::Grow { min, max } if main_max.is_finite() => {
                        let min = min.max(T::ZERO);
                        if max > min && max.is_finite() {
                            let capacity = max - min;
                            caps[cap_index] = (capacity.to_f32() / item.weight, capacity, item.weight);
                            cap_index += 1;
                        }
                    }
                    _ => {
                        let cross = self.resolve(cx, child, item, cross_axis);
                        let cross = self.cross_bounds(cross, cross_max);
                        self.layout_child(cx, child, main, cross, &mut space);
                    }
                }
            }
            if grow {
                let unit = Self::grow_unit(&mut caps, space.remaining, weights);
                self.grow_children(cx, unit, |cx, child, main, cross| {
                    let cross = self.cross_bounds(cross, cross_max);
                    self.layout_child(cx, child, (main, main), cross, &mut space);
                });
            }
        } else {
            // establish natural cross sizes and reserve fixed main sizes
            let mut cap_index = 0;
            let mut percentages = 0.0;
            let mut content_fill = false;
            let mut fixed_fill = false;
            for child in cx.children() {
                let item = cx.item(child);
                let sizing = self.resolve(cx, child, item, self.axis);
                let cross = self.resolve(cx, child, item, cross_axis);
                let main = self.main_bounds(sizing, &space, &mut percentages);
                match sizing {
                    Sizing::Grow { min, max } if main_max.is_finite() => {
                        let min = min.max(T::ZERO);
                        if max > min && max.is_finite() {
                            let capacity = max - min;
                            caps[cap_index] = (capacity.to_f32() / item.weight, capacity, item.weight);
                            cap_index += 1;
                        }
                    }
                    Sizing::Fixed(main) if self.fills_cross(cross) => {
                        if space.remaining.is_finite() {
                            space.remaining = (space.remaining - main.max(T::ZERO)).max(T::ZERO);
                        }
                        fixed_fill = true;
                    }
                    _ if self.fills_cross(cross) => content_fill = true,
                    _ => {
                        let cross = sizing_range(cross, cross_max);
                        self.layout_child(cx, child, main, cross, &mut space);
                    }
                }
            }

            // these children need measurement before leftover main space can be shared
            if content_fill {
                let mut percentages = 0.0;
                for child in cx.children() {
                    let item = cx.item(child);
                    let sizing = self.resolve(cx, child, item, self.axis);
                    let cross = self.resolve(cx, child, item, cross_axis);
                    let main = self.main_bounds(sizing, &space, &mut percentages);
                    match sizing {
                        Sizing::Fixed(_) => {}
                        Sizing::Grow { .. } if main_max.is_finite() => {}
                        _ if self.fills_cross(cross) => {
                            let extent = cross.clamp(space.cross);
                            self.layout_child(cx, child, main, (extent, extent), &mut space);
                        }
                        _ => {}
                    }
                }
            }

            if grow {
                let unit = Self::grow_unit(&mut caps, space.remaining, weights);
                // grow children with natural cross sizes establish the fill extent
                self.grow_children(cx, unit, |cx, child, main, cross| {
                    if !self.fills_cross(cross) {
                        let cross = sizing_range(cross, cross_max);
                        self.layout_child(cx, child, (main, main), cross, &mut space);
                    }
                });
                self.grow_children(cx, unit, |cx, child, main, cross| {
                    if self.fills_cross(cross) {
                        let extent = cross.clamp(space.cross);
                        self.layout_child(cx, child, (main, main), (extent, extent), &mut space);
                    }
                });
            }

            // fixed main sizes were reserved earlier and can now fill across
            if fixed_fill {
                for child in cx.children() {
                    let item = cx.item(child);
                    let cross = self.resolve(cx, child, item, cross_axis);
                    match self.resolve(cx, child, item, self.axis) {
                        Sizing::Fixed(main) if self.fills_cross(cross) => {
                            let main = main.max(T::ZERO);
                            let extent = cross.clamp(space.cross);
                            self.layout_child(cx, child, (main, main), (extent, extent), &mut space);
                        }
                        _ => {}
                    }
                }
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
    fn grow_unit(caps: &mut [(f32, T, f32)], mut remaining: T, mut weights: f32) -> f32 {
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
        unit
    }

    fn grow_children<C: Context<Scalar = T>>(
        &self,
        cx: &mut LayoutCx<'_, C, Item<T>>,
        unit: f32,
        mut layout: impl FnMut(&mut LayoutCx<'_, C, Item<T>>, blit::NodeId, T, Sizing<T>),
    ) {
        let mut allocation = 0.0;
        for child in cx.children() {
            let item = cx.item(child);
            let Sizing::Grow { min, max } = self.resolve(cx, child, item, self.axis) else {
                continue;
            };
            let min = min.max(T::ZERO);
            let capacity = if max == T::UNBOUNDED {
                f32::INFINITY
            } else {
                (max.max(min) - min).to_f32()
            };
            let main = min + T::allocate(&mut allocation, (unit * item.weight).min(capacity));
            let cross = self.resolve(cx, child, item, self.axis.other());
            layout(cx, child, main, cross);
        }
    }

    #[inline(always)]
    fn layout_child<C: Context<Scalar = T>>(
        &self,
        cx: &mut LayoutCx<'_, C, Item<T>>,
        child: blit::NodeId,
        main: (T, T),
        cross: (T, T),
        space: &mut Space<T>,
    ) {
        let size = cx.layout_child(child, flow_constraints(self.axis, main, cross));
        let main = self.axis.extent(size);
        space.used += main;
        if space.remaining.is_finite() {
            space.remaining = (space.remaining - main).max(T::ZERO);
        }
        space.cross = space.cross.max(self.axis.other().extent(size));
    }

    #[inline(always)]
    fn resolve<C: Context<Scalar = T>>(
        &self,
        cx: &LayoutCx<'_, C, Item<T>>,
        child: blit::NodeId,
        item: &Item<T>,
        axis: Axis,
    ) -> Sizing<T> {
        let sizing = item.sizing(axis).unwrap_or_else(|| cx.child_sizing(child, axis));
        super::resolve_sizing(cx, child, axis, sizing)
    }

    #[inline(always)]
    fn fills_cross(&self, sizing: Sizing<T>) -> bool {
        matches!(
            (sizing, self.align),
            (Sizing::Grow { .. }, _) | (Sizing::Fit { .. }, Align::Stretch)
        )
    }

    #[inline(always)]
    fn cross_bounds(&self, sizing: Sizing<T>, maximum: T) -> (T, T) {
        if self.fills_cross(sizing) {
            let extent = sizing.clamp(maximum);
            (extent, extent)
        } else {
            sizing_range(sizing, maximum)
        }
    }

    #[inline(always)]
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
}
