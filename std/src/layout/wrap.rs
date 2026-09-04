use blit::{Axis, Constraints, LayoutCx, Platform, Point, Sides, Size, Sizing};

pub use super::sizing::{Item, item};
use super::{
    Align, Justify, capped_growth, flow_constraints, flow_size, justify_offset, override_sizing,
    percentage, size_on_axis, sizing_range,
};

blit::builder! {
    /// lays out children in horizontal or vertical runs
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout {
        new(axis: Axis),
        padding: Sides = Sides::all(0.0),
        item_gap: f32 = 0.0,
        run_gap: f32 = 0.0,
        align: Align = Align::Start,
        justify: Justify = Justify::Start,
    }
}

impl Layout {
    pub const fn gap(mut self, gap: f32) -> Self {
        self.item_gap = gap;
        self.run_gap = gap;
        self
    }
}

pub fn layout(axis: Axis) -> Layout {
    Layout::new(axis)
}

pub fn horizontal() -> Layout {
    layout(Axis::Horizontal)
}

pub fn vertical() -> Layout {
    layout(Axis::Vertical)
}

impl<P: Platform> blit::Layout<P> for Layout {
    type Item = Item;

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, constraints: Constraints) -> Size {
        let res = cx.resolution();
        let padding = res.sides(self.padding);
        let cross_axis = match self.axis {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        };
        let (main_padding, cross_padding, main_leading, cross_leading) = match self.axis {
            Axis::Horizontal => (
                padding.left + padding.right,
                padding.top + padding.bottom,
                padding.left,
                padding.top,
            ),
            Axis::Vertical => (
                padding.top + padding.bottom,
                padding.left + padding.right,
                padding.top,
                padding.left,
            ),
        };
        let max_cross = (size_on_axis(constraints.max, cross_axis) - cross_padding).max(0.0);
        let item_gap = res.extent(self.axis, self.item_gap).max(0.0);
        let run_gap = res.extent(cross_axis, self.run_gap).max(0.0);
        let mut natural_main = 0.0;
        let mut target_natural_main = 0.0;
        let mut has_main_grow = false;
        let mut count = 0usize;

        for child in cx.children() {
            count += 1;
            let item = cx.item(child);
            let main_sizing = res.sizing(self.axis, item.sizing(self.axis));
            has_main_grow |= matches!(main_sizing, Sizing::Grow { .. });
            let cross_sizing = res.sizing(cross_axis, item.sizing(cross_axis));
            let size = cx.layout_child(
                child,
                flow_constraints(
                    self.axis,
                    sizing_range(main_sizing, f32::INFINITY),
                    sizing_range(cross_sizing, max_cross),
                ),
            );
            natural_main += size_on_axis(size, self.axis);
            target_natural_main += size_on_axis(cx.target_child_size(child), self.axis);
        }
        if count == 0 {
            return constraints.constrain(padding.size());
        }
        let spacing = item_gap * count.saturating_sub(1) as f32 + main_padding;
        natural_main += spacing;
        target_natural_main += spacing;
        let mut size = constraints.constrain(flow_size(natural_main, cross_padding, self.axis));
        let available_main = (size_on_axis(size, self.axis) - main_padding).max(0.0);
        let target_size =
            constraints.constrain(flow_size(target_natural_main, cross_padding, self.axis));
        let target_available_main = (size_on_axis(target_size, self.axis) - main_padding).max(0.0);

        let mut run_main: f32 = 0.0;
        let mut max_run_main: f32 = 0.0;
        let mut run_items = 0usize;
        for child in cx.children() {
            let natural = cx.child_size(child);
            let item = cx.item(child);
            let main_sizing = res.sizing(self.axis, item.sizing(self.axis));
            let cross_sizing = res.sizing(cross_axis, item.sizing(cross_axis));
            let main = match main_sizing {
                Sizing::Fit { .. } | Sizing::Grow { .. } => {
                    main_sizing.clamp(size_on_axis(natural, self.axis).min(available_main))
                }
                Sizing::Fixed(size) => size.max(0.0),
                Sizing::Percent(fraction) => percentage(fraction, available_main),
            };
            let cross = sizing_range(cross_sizing, max_cross);
            if main != size_on_axis(natural, self.axis) {
                cx.layout_child(child, flow_constraints(self.axis, (main, main), cross));
            }
            let child_main = size_on_axis(cx.child_size(child), self.axis);
            let needed = child_main + if run_items == 0 { 0.0 } else { item_gap };
            if run_items != 0 && run_main + needed > available_main {
                max_run_main = max_run_main.max(run_main);
                run_main = 0.0;
                run_items = 0;
            }
            run_main += child_main + if run_items == 0 { 0.0 } else { item_gap };
            run_items += 1;
        }
        if run_items != 0 {
            max_run_main = max_run_main.max(run_main);
        }

        size = constraints.constrain(flow_size(
            max_run_main + main_padding,
            cross_padding,
            self.axis,
        ));
        let available_main = (size_on_axis(size, self.axis) - main_padding).max(0.0);

        let mut children = cx.children().peekable();
        let mut cross_cursor = cross_leading;
        let mut final_main: f32 = 0.0;
        let mut final_cross: f32 = 0.0;
        let mut runs = 0usize;
        while children.peek().is_some() {
            let run = children.clone();
            let mut run_count = 0usize;
            let mut target_run_main = 0.0;
            let mut run_main = 0.0;
            let mut run_cross: f32 = 0.0;
            while let Some(&child) = children.peek() {
                let target_main = size_on_axis(cx.target_child_size(child), self.axis);
                let target_needed = target_main + if run_count == 0 { 0.0 } else { item_gap };
                let target_next = target_run_main + target_needed;
                let target_tolerance =
                    f32::EPSILON * target_available_main * (run_count + 1) as f32;
                if run_count != 0 && target_next - target_available_main > target_tolerance {
                    break;
                }
                children.next();
                target_run_main = target_next;
                let current = cx.child_size(child);
                run_main +=
                    size_on_axis(current, self.axis) + if run_count == 0 { 0.0 } else { item_gap };
                run_cross = run_cross.max(size_on_axis(current, cross_axis));
                run_count += 1;
            }

            if has_main_grow {
                let remaining = (available_main - run_main).max(0.0);
                let mut grow = 0usize;
                let mut minimum_growth = f32::INFINITY;
                for child in run.clone().take(run_count) {
                    let sizing = res.sizing(self.axis, cx.item(child).sizing(self.axis));
                    if matches!(sizing, Sizing::Grow { .. }) {
                        let current = size_on_axis(cx.child_size(child), self.axis);
                        let capacity = (sizing.clamp(f32::INFINITY) - current).max(0.0);
                        if capacity > 0.0 {
                            grow += 1;
                            minimum_growth = minimum_growth.min(capacity);
                        }
                    }
                }
                let growth = capped_growth(
                    remaining,
                    grow,
                    minimum_growth,
                    run.clone().take(run_count).filter_map(|child| {
                        let sizing = res.sizing(self.axis, cx.item(child).sizing(self.axis));
                        matches!(sizing, Sizing::Grow { .. }).then(|| {
                            let current = size_on_axis(cx.child_size(child), self.axis);
                            (sizing.clamp(f32::INFINITY) - current).max(0.0)
                        })
                    }),
                );

                if growth > 0.0 {
                    run_main = item_gap * run_count.saturating_sub(1) as f32;
                    run_cross = 0.0;
                    for child in run.clone().take(run_count) {
                        let item = cx.item(child);
                        let main_sizing = res.sizing(self.axis, item.sizing(self.axis));
                        if matches!(main_sizing, Sizing::Grow { .. }) {
                            let current = size_on_axis(cx.child_size(child), self.axis);
                            let capacity = (main_sizing.clamp(f32::INFINITY) - current).max(0.0);
                            let main = current + growth.min(capacity);
                            if main != current {
                                let cross_sizing = res.sizing(cross_axis, item.sizing(cross_axis));
                                cx.layout_child(
                                    child,
                                    flow_constraints(
                                        self.axis,
                                        (main, main),
                                        sizing_range(cross_sizing, max_cross),
                                    ),
                                );
                            }
                        }
                        let size = cx.child_size(child);
                        run_main += size_on_axis(size, self.axis);
                        run_cross = run_cross.max(size_on_axis(size, cross_axis));
                    }
                }
            }

            final_main = final_main.max(run_main);
            final_cross += run_cross + if runs == 0 { 0.0 } else { run_gap };
            runs += 1;
            let remaining = (available_main - run_main).max(0.0);
            let (offset, extra_gap) = justify_offset(self.justify, remaining, run_count);
            let mut main_cursor = main_leading + offset;
            for child in run.take(run_count) {
                let child_size = cx.child_size(child);
                let child_main = size_on_axis(child_size, self.axis);
                let current_cross = size_on_axis(child_size, cross_axis);
                let cross_sizing = res.sizing(cross_axis, cx.item(child).sizing(cross_axis));
                if matches!(cross_sizing, Sizing::Grow { .. })
                    || self.align == Align::Stretch && matches!(cross_sizing, Sizing::Fit { .. })
                {
                    let cross = cross_sizing.clamp(run_cross);
                    if cross != current_cross {
                        cx.layout_child(
                            child,
                            flow_constraints(self.axis, (child_main, child_main), (cross, cross)),
                        );
                    }
                }
                let child_cross = size_on_axis(cx.child_size(child), cross_axis);
                let cross_offset = match self.align {
                    Align::Start | Align::Stretch => 0.0,
                    Align::Center => (run_cross - child_cross).max(0.0) / 2.0,
                    Align::End => (run_cross - child_cross).max(0.0),
                };
                let position = flow_size(main_cursor, cross_cursor + cross_offset, self.axis);
                cx.set_child_position(child, Point::new(position.width, position.height));
                main_cursor += child_main + item_gap + extra_gap;
            }
            cross_cursor += run_cross + run_gap;
        }

        constraints.constrain(flow_size(
            final_main + main_padding,
            final_cross + cross_padding,
            self.axis,
        ))
    }

    fn override_size(
        &self,
        item: &mut Self::Item,
        width: Option<f32>,
        height: Option<f32>,
    ) -> bool {
        override_sizing(&mut item.width, &mut item.height, width, height)
    }
}
