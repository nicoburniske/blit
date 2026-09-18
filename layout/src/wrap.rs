use blit::{Axis, Constraints, LayoutCx, Platform, Point, Sides, Size, Sizing};

pub use super::sizing::{Item, item};
use super::{
    Align, Justify, flow_constraints, flow_size, justify_offset, override_sizing, sizing_range,
};

blit::builder! {
    /// wraps children into rows or columns
    ///
    /// natural sizes choose the runs, then grow children share the space left in each run
    /// if a grow child reaches its limit, the unused space is left for justification
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

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, bounds: Constraints) -> Size {
        let res = cx.resolution();
        let padding = res.sides(self.padding);
        let cross_axis = self.axis.other();
        let main_padding = self.axis.extent(padding.size());
        let cross_padding = cross_axis.extent(padding.size());
        let leading = Size::new(padding.left, padding.top);
        let main_leading = self.axis.extent(leading);
        let cross_leading = cross_axis.extent(leading);
        let main_max = (self.axis.extent(bounds.max) - main_padding).max(0.0);
        let cross_max = (cross_axis.extent(bounds.max) - cross_padding).max(0.0);
        let item_gap = res.extent(self.axis, self.item_gap).max(0.0);
        let run_gap = res.extent(cross_axis, self.run_gap).max(0.0);

        // measure natural child sizes
        for child in cx.children() {
            let item = cx.item(child);
            cx.layout_child(
                child,
                flow_constraints(
                    self.axis,
                    sizing_range(res.sizing(self.axis, item.sizing(self.axis)), main_max),
                    sizing_range(res.sizing(cross_axis, item.sizing(cross_axis)), cross_max),
                ),
            );
        }

        // shrinkwrap the longest current run
        let mut longest: f32 = 0.0;
        let mut run: f32 = 0.0;
        let mut count = 0usize;
        for child in cx.children() {
            let child = self.axis.extent(cx.child_size(child));
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
        let available = (self.axis.extent(size) - main_padding).max(0.0);

        // preserve target runs during size transitions
        let mut target_longest: f32 = 0.0;
        let mut target_run: f32 = 0.0;
        let mut target_count = 0usize;
        for child in cx.children() {
            let child = self.axis.extent(cx.target_child_size(child));
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
        let target_size = bounds.constrain(flow_size(
            target_longest + main_padding,
            cross_padding,
            self.axis,
        ));
        let target_available = (self.axis.extent(target_size) - main_padding).max(0.0);

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
                let child = self.axis.extent(cx.target_child_size(child));
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
                main += self.axis.extent(cx.child_size(child));
                grows += usize::from(matches!(
                    res.sizing(self.axis, cx.item(child).sizing(self.axis)),
                    Sizing::Grow { .. }
                ));
            }
            let growth = if grows == 0 {
                0.0
            } else {
                (available - main).max(0.0) / grows as f32
            };

            // apply main growth and find the run cross extent
            main = item_gap * count.saturating_sub(1) as f32;
            let mut cross: f32 = 0.0;
            for child in start.clone().take(count) {
                let item = cx.item(child);
                let main_sizing = res.sizing(self.axis, item.sizing(self.axis));
                let current = cx.child_size(child);
                let current_main = self.axis.extent(current);
                if matches!(main_sizing, Sizing::Grow { .. }) {
                    let assigned = main_sizing.clamp(current_main + growth);
                    if assigned != current_main {
                        cx.layout_child(
                            child,
                            flow_constraints(
                                self.axis,
                                (assigned, assigned),
                                sizing_range(
                                    res.sizing(cross_axis, item.sizing(cross_axis)),
                                    cross_max,
                                ),
                            ),
                        );
                    }
                }
                let child = cx.child_size(child);
                main += self.axis.extent(child);
                cross = cross.max(cross_axis.extent(child));
            }

            // stretch and position the completed run
            let (offset, extra_gap) =
                justify_offset(self.justify, (available - main).max(0.0), count);
            let mut main_cursor = main_leading + offset;
            for child in start.take(count) {
                let item = cx.item(child);
                let child_size = cx.child_size(child);
                let child_main = self.axis.extent(child_size);
                let child_cross = cross_axis.extent(child_size);
                let cross_sizing = res.sizing(cross_axis, item.sizing(cross_axis));
                if matches!(cross_sizing, Sizing::Grow { .. })
                    || self.align == Align::Stretch && matches!(cross_sizing, Sizing::Fit { .. })
                {
                    let assigned = cross_sizing.clamp(cross);
                    if assigned != child_cross {
                        cx.layout_child(
                            child,
                            flow_constraints(
                                self.axis,
                                (child_main, child_main),
                                (assigned, assigned),
                            ),
                        );
                    }
                }
                let child_cross = cross_axis.extent(cx.child_size(child));
                let cross_offset = match self.align {
                    Align::Start | Align::Stretch => 0.0,
                    Align::Center => (cross - child_cross).max(0.0) / 2.0,
                    Align::End => (cross - child_cross).max(0.0),
                };
                let position = flow_size(main_cursor, cross_cursor + cross_offset, self.axis);
                cx.set_child_position(child, Point::new(position.width, position.height));
                main_cursor += child_main + item_gap + extra_gap;
            }

            occupied_cross += cross + if runs == 0 { 0.0 } else { run_gap };
            cross_cursor += cross + run_gap;
            runs += 1;
        }

        let main = self.axis.extent(size);
        bounds.constrain(flow_size(main, occupied_cross + cross_padding, self.axis))
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
