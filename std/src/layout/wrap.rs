use blit::{Axis, Constraints, Layout, LayoutCx, Platform, Point, Sides, Size, Sizing};

use super::{
    Align, Justify, flow_constraints, flow_size, justify_offset, override_sizing, percentage,
    size_on_axis, sizing_range,
};

blit::builder! {
    /// sizing policy for a wrapping child
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct WrapItem {
        new(),
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
    }
}

impl WrapItem {
    pub fn fixed(width: f32, height: f32) -> Self {
        Self::new()
            .width(Sizing::fixed(width))
            .height(Sizing::fixed(height))
    }

    pub fn grow() -> Self {
        Self::new().width(Sizing::grow()).height(Sizing::grow())
    }

    fn sizing(self, axis: Axis) -> Sizing {
        match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => self.height,
        }
    }
}

blit::builder! {
    /// lays out children in horizontal or vertical runs
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Wrap {
        new(axis: Axis),
        padding: Sides = Sides::all(0.0),
        item_gap: f32 = 0.0,
        run_gap: f32 = 0.0,
        align: Align = Align::Start,
        justify: Justify = Justify::Start,
    }
}

impl Wrap {
    pub fn horizontal() -> Self {
        Self::new(Axis::Horizontal)
    }

    pub fn vertical() -> Self {
        Self::new(Axis::Vertical)
    }

    pub const fn gap(mut self, gap: f32) -> Self {
        self.item_gap = gap;
        self.run_gap = gap;
        self
    }
}

impl<P: Platform> Layout<P> for Wrap {
    type Item = WrapItem;

    fn size_override(&self, item: &mut Self::Item, width: Option<f32>, height: Option<f32>) {
        override_sizing(&mut item.width, &mut item.height, width, height);
    }

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, constraints: Constraints) -> Size {
        let res = cx.layout_resolution();
        let cross_axis = match self.axis {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        };
        let count = cx.children().count();
        if count == 0 {
            return constraints.constrain(Size::default());
        }

        let padding = res.sides(self.padding);
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

        for node in cx.children() {
            let item = cx.item(node);
            let main_sizing = res.sizing(self.axis, item.sizing(self.axis));
            let cross_sizing = res.sizing(cross_axis, item.sizing(cross_axis));
            let size = cx.layout_child(
                node,
                flow_constraints(
                    self.axis,
                    sizing_range(main_sizing, f32::INFINITY),
                    sizing_range(cross_sizing, max_cross),
                ),
            );
            natural_main += size_on_axis(size, self.axis);
        }
        natural_main += item_gap * count.saturating_sub(1) as f32 + main_padding;
        let mut size = constraints.constrain(flow_size(natural_main, cross_padding, self.axis));
        let available_main = (size_on_axis(size, self.axis) - main_padding).max(0.0);

        for node in cx.children() {
            let natural = cx.size(node);
            let item = cx.item(node);
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
                cx.constrain_child(node, flow_constraints(self.axis, (main, main), cross));
            }
        }

        let mut run_main: f32 = 0.0;
        let mut run_cross: f32 = 0.0;
        let mut total_cross: f32 = 0.0;
        let mut max_run_main: f32 = 0.0;
        let mut run_items = 0usize;
        let mut runs = 0usize;
        for node in cx.children() {
            let child_main = cx.axis_size(node, self.axis);
            let needed = child_main + if run_items == 0 { 0.0 } else { item_gap };
            if run_items != 0 && run_main + needed > available_main {
                max_run_main = max_run_main.max(run_main);
                total_cross += run_cross + if runs == 0 { 0.0 } else { run_gap };
                runs += 1;
                run_main = 0.0;
                run_cross = 0.0;
                run_items = 0;
            }
            run_main += child_main + if run_items == 0 { 0.0 } else { item_gap };
            run_cross = run_cross.max(cx.axis_size(node, cross_axis));
            run_items += 1;
        }
        if run_items != 0 {
            max_run_main = max_run_main.max(run_main);
            total_cross += run_cross + if runs == 0 { 0.0 } else { run_gap };
        }

        size = constraints.constrain(flow_size(
            max_run_main + main_padding,
            total_cross + cross_padding,
            self.axis,
        ));
        let available_main = (size_on_axis(size, self.axis) - main_padding).max(0.0);

        let mut children = cx.children().peekable();
        let mut cross_cursor = cross_leading;
        while children.peek().is_some() {
            let run = children.clone();
            let mut run_count = 0usize;
            let mut run_main = 0.0;
            let mut run_cross: f32 = 0.0;
            while let Some(&node) = children.peek() {
                let child_main = cx.axis_size(node, self.axis);
                let needed = child_main + if run_count == 0 { 0.0 } else { item_gap };
                if run_count != 0 && run_main + needed > available_main {
                    break;
                }
                children.next();
                run_main += needed;
                run_cross = run_cross.max(cx.axis_size(node, cross_axis));
                run_count += 1;
            }

            let remaining = (available_main - run_main).max(0.0);
            let (offset, extra_gap) = justify_offset(self.justify, remaining, run_count);
            let mut main_cursor = main_leading + offset;
            for node in run.take(run_count) {
                let child_main = cx.axis_size(node, self.axis);
                let cross_sizing = res.sizing(cross_axis, cx.item(node).sizing(cross_axis));
                if self.align == Align::Stretch
                    && matches!(cross_sizing, Sizing::Fit { .. } | Sizing::Grow { .. })
                {
                    let cross = cross_sizing.clamp(run_cross);
                    if cross != cx.axis_size(node, cross_axis) {
                        cx.constrain_child(
                            node,
                            flow_constraints(self.axis, (child_main, child_main), (cross, cross)),
                        );
                    }
                }
                let child_cross = cx.axis_size(node, cross_axis);
                let cross_offset = match self.align {
                    Align::Start | Align::Stretch => 0.0,
                    Align::Center => (run_cross - child_cross).max(0.0) / 2.0,
                    Align::End => (run_cross - child_cross).max(0.0),
                };
                let position = flow_size(main_cursor, cross_cursor + cross_offset, self.axis);
                cx.set_position(
                    node,
                    Point {
                        x: position.width,
                        y: position.height,
                    },
                );
                main_cursor += child_main + item_gap + extra_gap;
            }
            cross_cursor += run_cross + run_gap;
        }

        size
    }
}
