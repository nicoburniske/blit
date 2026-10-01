use blit::{Axis, Constraints, IntrinsicQuery, IntrinsicSize, LayoutCx, LogicalPoint, LogicalSize, MeasureCx};

pub use crate::size::Item;
use crate::{
    Align, Justify, Padding, Sizing, Unit, flow_constraints, flow_size, flow_sizing, intrinsic_child, intrinsic_range,
    justify_offset, size::axis_sizing, sizing_range,
};

blit::builder! {
    #[const]
    /// wraps children into rows or columns
    ///
    /// natural sizes choose the runs, then grow children share the space left in each run
    /// if a grow child reaches its limit, the unused space is left for justification
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout<U: Unit> {
        new(axis: Axis),
        padding: Padding<U> = Padding::all(U::ZERO),
        item_gap: U = U::ZERO,
        run_gap: U = U::ZERO,
        align: Align = Align::Start,
        justify: Justify = Justify::Start,
    }
}

impl<U: Unit> Layout<U> {
    pub const fn gap(mut self, gap: U) -> Self {
        self.item_gap = gap;
        self.run_gap = gap;
        self
    }
}

impl<U: Unit> blit::Layout for Layout<U> {
    type Item = Item<U>;

    fn intrinsic(&self, cx: &mut MeasureCx<'_, Self::Item>, query: IntrinsicQuery) -> IntrinsicSize {
        let padding: blit::Sides = self.padding.into();
        let extent = query.axis.extent(padding.size());
        let item_gap = self.item_gap.into_float().max(0.0);
        let run_gap = self.run_gap.into_float().max(0.0);
        if query.axis == self.axis {
            let mut result = IntrinsicSize::uniform(extent);
            let mut count = 0;
            for child in cx.children() {
                let (main, cross) = flow_sizing(self.axis, child.item.width, child.item.height, (None, None));
                let cross = match cross {
                    Sizing::Grow { min, max } => Sizing::Fit { min, max },
                    cross => cross,
                };
                let size = intrinsic_child::<_, U>(
                    cx,
                    child.id,
                    IntrinsicQuery {
                        axis: query.axis,
                        cross: query
                            .cross
                            .map(|value| (value - query.axis.other().extent(padding.size())).max(0.0)),
                    },
                    main,
                    cross,
                    false,
                );
                result.min = result.min.max(U::round(size.min) + extent);
                result.preferred += U::round(size.preferred) + if count == 0 { 0.0 } else { item_gap };
                count += 1;
            }
            return result;
        }
        let available = query.cross.map_or(f32::INFINITY, |value| {
            (U::round(value) - self.axis.extent(padding.size())).max(0.0)
        });
        let mut widths = cx.scratch(cx.children().count(), 0.0f32);
        let mut percentages = 0.0;
        for (index, child) in cx.children().enumerate() {
            let (mut main, cross) = flow_sizing(self.axis, child.item.width, child.item.height, (None, None));
            if matches!(main, Sizing::Percent(_)) && !available.is_finite() {
                main = Sizing::fit();
            }
            let range = sizing_range::<U>(main, available);
            let width = if matches!(main, Sizing::Fixed(_) | Sizing::Percent(_)) {
                U::distribute(&mut percentages, range.0)
            } else {
                let size = intrinsic_child::<_, U>(cx, child.id, IntrinsicQuery::new(self.axis), main, cross, false);
                U::round(main.clamp(size.preferred)).clamp(range.0, range.1)
            };
            cx.scratch_mut(&mut widths)[index] = width;
        }
        let mut children = cx.children().enumerate();
        let mut result = IntrinsicSize::uniform(extent);
        let mut runs = 0;
        let mut next = 0;
        while next < cx.scratch_mut(&mut widths).len() {
            let run = next_run(cx.scratch_mut(&mut widths), next, available, item_gap, 0.0);
            let count = run.end - next;
            grow_run::<U>(
                &mut cx.scratch_mut(&mut widths)[next..run.end],
                if available.is_finite() { available } else { run.used },
                run.used,
                children
                    .clone()
                    .take(count)
                    .map(|(_, child)| axis_sizing(self.axis, child.item.width, child.item.height, (None, None))),
            );
            next = run.end;
            let mut run = IntrinsicSize::default();
            for (index, child) in children.by_ref().take(count) {
                let cross = axis_sizing(query.axis, child.item.width, child.item.height, (None, None));
                let width = cx.scratch_mut(&mut widths)[index];
                let size = if let Sizing::Fixed(value) = cross {
                    IntrinsicSize::uniform(value.max(0.0))
                } else {
                    cx.intrinsic(child.id, IntrinsicQuery::new(query.axis).cross(width))
                };
                let size = intrinsic_range(cross, size);
                run.min = run.min.max(U::round(size.min));
                run.preferred = run.preferred.max(U::round(size.preferred));
            }
            result.min += run.min + if runs == 0 { 0.0 } else { run_gap };
            result.preferred += run.preferred + if runs == 0 { 0.0 } else { run_gap };
            runs += 1;
        }
        result
    }

    fn layout(&self, cx: &mut LayoutCx<'_, Self::Item>, bounds: Constraints) -> LogicalSize {
        let padding: blit::Sides = self.padding.into();
        let cross_axis = self.axis.other();
        let main_padding = self.axis.extent(padding.size());
        let cross_padding = cross_axis.extent(padding.size());
        let leading = LogicalSize::new(padding.left, padding.top);
        let main_leading = self.axis.extent(leading);
        let cross_leading = cross_axis.extent(leading);
        let main_max = (U::round(self.axis.extent(bounds.max)) - main_padding).max(0.0);
        let cross_max = (U::round(cross_axis.extent(bounds.max)) - cross_padding).max(0.0);
        let item_gap = self.item_gap.into_float().max(0.0);
        let run_gap = self.run_gap.into_float().max(0.0);

        // measure natural child sizes
        let mut percentages = 0.0;
        for child in cx.children() {
            let item = child.item;
            let (main, cross) = flow_sizing(self.axis, item.width, item.height, cx.size_overrides(child.id));
            let mut main_bounds = sizing_range::<U>(main, main_max);
            if matches!(main, Sizing::Percent(_)) {
                let extent = U::distribute(&mut percentages, main_bounds.0);
                main_bounds = (extent, extent);
            }
            cx.layout_child(
                child.id,
                flow_constraints::<U>(self.axis, main_bounds, sizing_range::<U>(cross, cross_max)),
            );
        }

        let count = cx.children().count();
        let mut widths = cx.scratch(count, 0.0f32);
        let mut targets = cx.scratch(count, 0.0f32);
        for (index, child) in cx.children().enumerate() {
            let current = U::round(self.axis.extent(cx.size(child.id)));
            let target = U::round(self.axis.extent(cx.target_size(child.id)));
            cx.scratch_mut(&mut widths)[index] = current;
            cx.scratch_mut(&mut targets)[index] = target;
        }
        let longest = longest_run(cx.scratch_mut(&mut widths), main_max, item_gap);
        let size = bounds.constrain(flow_size(longest + main_padding, cross_padding, self.axis));
        let available = (U::round(self.axis.extent(size)) - main_padding).max(0.0);

        let target_longest = longest_run(cx.scratch_mut(&mut targets), main_max, item_gap);
        let target_size = bounds.constrain(flow_size(target_longest + main_padding, cross_padding, self.axis));
        let target_available = (U::round(self.axis.extent(target_size)) - main_padding).max(0.0);

        let mut children = cx.children();
        let mut cross_cursor = cross_leading;
        let mut occupied_cross: f32 = 0.0;
        let mut runs = 0usize;
        let mut next = 0;
        while next < count {
            let start = children.clone();
            let run = next_run(
                cx.scratch_mut(&mut targets),
                next,
                target_available,
                item_gap,
                f32::EPSILON * target_available,
            );
            let count = run.end - next;
            let run_widths = &mut cx.scratch_mut(&mut widths)[next..run.end];
            let used = run_widths
                .iter()
                .fold(item_gap * count.saturating_sub(1) as f32, |used, width| used + width);
            let sizing = start.clone().take(count).map(|child| {
                let item = child.item;
                axis_sizing(self.axis, item.width, item.height, cx.size_overrides(child.id)).map(U::round)
            });
            grow_run::<U>(run_widths, available, used, sizing);
            next = run.end;

            // lay out assigned widths and find the run cross extent
            let mut main = item_gap * count.saturating_sub(1) as f32;
            let mut cross: f32 = 0.0;
            for (index, child) in start.clone().take(count).enumerate() {
                let item = child.item;
                let cross_sizing = axis_sizing(cross_axis, item.width, item.height, cx.size_overrides(child.id));
                let current = cx.size(child.id);
                let current_main = U::round(self.axis.extent(current));
                let assigned = cx.scratch_mut(&mut widths)[next - count + index];
                if assigned != current_main {
                    cx.layout_child(
                        child.id,
                        flow_constraints::<U>(
                            self.axis,
                            (assigned, assigned),
                            sizing_range::<U>(cross_sizing, cross_max),
                        ),
                    );
                }
                let child = cx.size(child.id);
                main += U::round(self.axis.extent(child));
                cross = cross.max(U::round(cross_axis.extent(child)));
            }

            // stretch and position the completed run
            let (offset, extra_gap) = justify_offset(self.justify, (available - main).max(0.0), count);
            let mut main_cursor = main_leading + offset;
            for child in children.by_ref().take(count) {
                let item = child.item;
                let child_size = cx.size(child.id);
                let child_main = U::round(self.axis.extent(child_size));
                let child_cross = U::round(cross_axis.extent(child_size));
                let cross_sizing = axis_sizing(cross_axis, item.width, item.height, cx.size_overrides(child.id));
                let cross_sizing = cross_sizing.map(U::round);
                if matches!(cross_sizing, Sizing::Grow { .. })
                    || self.align == Align::Stretch && matches!(cross_sizing, Sizing::Fit { .. })
                {
                    let assigned = cross_sizing.clamp(cross);
                    if assigned != child_cross {
                        cx.layout_child(
                            child.id,
                            flow_constraints::<U>(self.axis, (child_main, child_main), (assigned, assigned)),
                        );
                    }
                }
                let child_cross = U::round(cross_axis.extent(cx.size(child.id)));
                let cross_offset = match self.align {
                    Align::Start | Align::Stretch => 0.0,
                    Align::Center => (cross - child_cross).max(0.0) / 2.0,
                    Align::End => (cross - child_cross).max(0.0),
                };
                let position = flow_size(U::round(main_cursor), U::round(cross_cursor + cross_offset), self.axis);
                cx.set_position(child.id, LogicalPoint::new(position.width, position.height));
                main_cursor += child_main + item_gap + extra_gap;
            }

            occupied_cross += cross + if runs == 0 { 0.0 } else { run_gap };
            cross_cursor += cross + run_gap;
            runs += 1;
        }

        let main = U::round(self.axis.extent(size));
        bounds.constrain(flow_size(main, occupied_cross + cross_padding, self.axis))
    }
}

struct Run {
    end: usize,
    used: f32,
}

fn grow_run<U: Unit>(widths: &mut [f32], available: f32, used: f32, sizing: impl Iterator<Item = Sizing<f32>> + Clone) {
    let grows = sizing
        .clone()
        .filter(|sizing| matches!(sizing, Sizing::Grow { .. }))
        .count();
    let growth = if grows == 0 {
        0.0
    } else {
        (available - used).max(0.0) / grows as f32
    };
    let mut cursor = 0.0;
    for (width, sizing) in widths.iter_mut().zip(sizing) {
        if matches!(sizing, Sizing::Grow { .. }) {
            *width += U::distribute(&mut cursor, sizing.clamp(*width + growth) - *width);
        }
    }
}

#[inline]
fn next_run(widths: &[f32], start: usize, available: f32, gap: f32, tolerance: f32) -> Run {
    let mut run = Run { end: start, used: 0.0 };
    for &width in &widths[start..] {
        let count = run.end - start;
        let needed = width + if count == 0 { 0.0 } else { gap };
        if count != 0 && run.used + needed - available > tolerance * (count + 1) as f32 {
            break;
        }
        run.used += needed;
        run.end += 1;
    }
    run
}

#[inline]
fn longest_run(widths: &[f32], available: f32, gap: f32) -> f32 {
    let mut longest: f32 = 0.0;
    let mut next = 0;
    while next < widths.len() {
        let run = next_run(widths, next, available, gap, 0.0);
        longest = longest.max(run.used);
        next = run.end;
    }
    longest
}
