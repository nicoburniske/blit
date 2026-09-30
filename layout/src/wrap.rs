use blit::{Axis, Constraints, LayoutCx, LogicalPoint, LogicalSize};

pub use crate::size::Item;
use crate::{
    Align, Justify, Padding, Sizing, Unit, flow_constraints, flow_size, flow_sizing, justify_offset, sizing_range,
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

impl<C, U: Unit> blit::Layout<C> for Layout<U> {
    type Item = Item<U>;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> LogicalSize {
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
            let item = cx.item(child);
            let (main, cross) = flow_sizing(self.axis, item.width, item.height, cx.size_overrides(child));
            let mut main_bounds = sizing_range::<U>(main, main_max);
            if matches!(main, Sizing::Percent(_)) {
                let extent = U::distribute(&mut percentages, main_bounds.0);
                main_bounds = (extent, extent);
            }
            cx.layout_child(
                child,
                flow_constraints::<U>(self.axis, main_bounds, sizing_range::<U>(cross, cross_max)),
            );
        }

        // shrinkwrap the longest current run
        let mut longest: f32 = 0.0;
        let mut run: f32 = 0.0;
        let mut count = 0usize;
        for child in cx.children() {
            let child = U::round(self.axis.extent(cx.size(child)));
            let needed = child + if count == 0 { 0.0 } else { item_gap };
            if count != 0 && run + needed > main_max {
                longest = longest.max(run);
                run = child;
                count = 1;
            } else {
                run += needed;
                count += 1;
            }
        }
        longest = longest.max(run);
        let size = bounds.constrain(flow_size(longest + main_padding, cross_padding, self.axis));
        let available = (U::round(self.axis.extent(size)) - main_padding).max(0.0);

        // preserve target runs during size transitions
        let mut target_longest: f32 = 0.0;
        let mut target_run: f32 = 0.0;
        let mut target_count = 0usize;
        for child in cx.children() {
            let child = U::round(self.axis.extent(cx.target_size(child)));
            let needed = child + if target_count == 0 { 0.0 } else { item_gap };
            if target_count != 0 && target_run + needed > main_max {
                target_longest = target_longest.max(target_run);
                target_run = child;
                target_count = 1;
            } else {
                target_run += needed;
                target_count += 1;
            }
        }
        target_longest = target_longest.max(target_run);
        let target_size = bounds.constrain(flow_size(target_longest + main_padding, cross_padding, self.axis));
        let target_available = (U::round(self.axis.extent(target_size)) - main_padding).max(0.0);

        let mut children = cx.children().peekable();
        let mut cross_cursor = cross_leading;
        let mut occupied_cross: f32 = 0.0;
        let mut runs = 0usize;
        while children.peek().is_some() {
            // form the next target run
            let start = children.clone();
            let mut count = 0usize;
            let mut target_main = 0.0;
            while let Some(&child) = children.peek() {
                let child = U::round(self.axis.extent(cx.target_size(child)));
                let needed = child + if count == 0 { 0.0 } else { item_gap };
                let tolerance = f32::EPSILON * target_available * (count + 1) as f32;
                if count != 0 && target_main + needed - target_available > tolerance {
                    break;
                }
                children.next();
                target_main += needed;
                count += 1;
            }

            // divide spare space equally among grow children
            let mut main = item_gap * count.saturating_sub(1) as f32;
            let mut grows = 0usize;
            for child in start.clone().take(count) {
                main += U::round(self.axis.extent(cx.size(child)));
                let item = cx.item(child);
                let (width, height) = cx.size_overrides(child);
                let sizing = match self.axis {
                    Axis::Horizontal => item.width.with_override(width),
                    Axis::Vertical => item.height.with_override(height),
                };
                grows += usize::from(matches!(sizing, Sizing::Grow { .. }));
            }
            let growth = if grows == 0 {
                0.0
            } else {
                (available - main).max(0.0) / grows as f32
            };

            // apply main growth and find the run cross extent
            main = item_gap * count.saturating_sub(1) as f32;
            let mut cross: f32 = 0.0;
            let mut growth_cursor: f32 = 0.0;
            for child in start.clone().take(count) {
                let item = cx.item(child);
                let (main_sizing, cross_sizing) =
                    flow_sizing(self.axis, item.width, item.height, cx.size_overrides(child));
                let main_sizing = main_sizing.map(U::round);
                let current = cx.size(child);
                let current_main = U::round(self.axis.extent(current));
                if matches!(main_sizing, Sizing::Grow { .. }) {
                    let target = main_sizing.clamp(current_main + growth);
                    let assigned = current_main + U::distribute(&mut growth_cursor, target - current_main);
                    if assigned != current_main {
                        cx.layout_child(
                            child,
                            flow_constraints::<U>(
                                self.axis,
                                (assigned, assigned),
                                sizing_range::<U>(cross_sizing, cross_max),
                            ),
                        );
                    }
                }
                let child = cx.size(child);
                main += U::round(self.axis.extent(child));
                cross = cross.max(U::round(cross_axis.extent(child)));
            }

            // stretch and position the completed run
            let (offset, extra_gap) = justify_offset(self.justify, (available - main).max(0.0), count);
            let mut main_cursor = main_leading + offset;
            for child in start.take(count) {
                let item = cx.item(child);
                let child_size = cx.size(child);
                let child_main = U::round(self.axis.extent(child_size));
                let child_cross = U::round(cross_axis.extent(child_size));
                let (width, height) = cx.size_overrides(child);
                let cross_sizing = match self.axis {
                    Axis::Horizontal => item.height.with_override(height),
                    Axis::Vertical => item.width.with_override(width),
                };
                let cross_sizing = cross_sizing.map(U::round);
                if matches!(cross_sizing, Sizing::Grow { .. })
                    || self.align == Align::Stretch && matches!(cross_sizing, Sizing::Fit { .. })
                {
                    let assigned = cross_sizing.clamp(cross);
                    if assigned != child_cross {
                        cx.layout_child(
                            child,
                            flow_constraints::<U>(self.axis, (child_main, child_main), (assigned, assigned)),
                        );
                    }
                }
                let child_cross = U::round(cross_axis.extent(cx.size(child)));
                let cross_offset = match self.align {
                    Align::Start | Align::Stretch => 0.0,
                    Align::Center => (cross - child_cross).max(0.0) / 2.0,
                    Align::End => (cross - child_cross).max(0.0),
                };
                let position = flow_size(U::round(main_cursor), U::round(cross_cursor + cross_offset), self.axis);
                cx.set_position(child, LogicalPoint::new(position.width, position.height));
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
