use blit::{Axis, Constraints, LayoutCx, Point, Sides, Size};

use super::{Align, Justify, allocated_range, flow_constraints, flow_size, justify_offset, sizing_range};
use crate::{Context, Sizing, resolve_sizing};

blit::builder! {
    /// wraps children into rows or columns
    ///
    /// measured sizes choose the runs, then grow children share the space left in each run
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

blit::builder! {
    /// sizing policy for a wrapping child
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Item {
        new(),
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
    }
}

impl Layout {
    pub const fn gap(mut self, gap: f32) -> Self {
        self.item_gap = gap;
        self.run_gap = gap;
        self
    }
}

impl Item {
    pub fn fixed(mut self, width: f32, height: f32) -> Self {
        self.width = Sizing::fixed(width);
        self.height = Sizing::fixed(height);
        self
    }

    pub fn grow(mut self) -> Self {
        self.width = Sizing::grow();
        self.height = Sizing::grow();
        self
    }

    pub fn sizing(&self, axis: Axis) -> Sizing {
        match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => self.height,
        }
    }
}

pub fn new(axis: Axis) -> Layout {
    Layout::new(axis)
}

pub fn horizontal() -> Layout {
    new(Axis::Horizontal)
}

pub fn vertical() -> Layout {
    new(Axis::Vertical)
}

pub fn item() -> Item {
    Item::new()
}

impl<C: Context> blit::Layout<C> for Layout {
    type Item = Item;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints) -> Size {
        let padding = crate::round_padding::<C>(self.padding);
        let cross_axis = self.axis.other();
        let main_padding = self.axis.extent(padding.size());
        let cross_padding = cross_axis.extent(padding.size());
        let leading = Size::new(padding.left, padding.top);
        let main_leading = self.axis.extent(leading);
        let cross_leading = cross_axis.extent(leading);
        let item_gap = C::round(self.item_gap).max(0.0);
        let run_gap = C::round(self.run_gap).max(0.0);
        let main_max = (self.axis.extent(bounds.max) - main_padding).max(0.0);
        let cross_max = (cross_axis.extent(bounds.max) - cross_padding).max(0.0);
        let mut percentages = 0.0;
        for child in cx.children() {
            let item = cx.item(child);
            let main = resolve_sizing(cx, child, self.axis, item.sizing(self.axis));
            cx.layout_child(
                child,
                flow_constraints(
                    self.axis,
                    allocated_range::<C>(main, main_max, &mut percentages),
                    sizing_range::<C>(
                        resolve_sizing(cx, child, cross_axis, item.sizing(cross_axis)),
                        cross_max,
                    ),
                ),
            );
        }
        let longest = longest_run(
            cx.children().map(|child| self.axis.extent(cx.size(child))),
            main_max,
            item_gap,
        );
        let size = bounds.constrain(flow_size(longest + main_padding, cross_padding, self.axis));
        let available = (self.axis.extent(size) - main_padding).max(0.0);
        let target_longest = longest_run(
            cx.children().map(|child| self.axis.extent(cx.target_size(child))),
            main_max,
            item_gap,
        );
        let target_size = bounds.constrain(flow_size(target_longest + main_padding, cross_padding, self.axis));
        let target_available = (self.axis.extent(target_size) - main_padding).max(0.0);
        let mut targets = cx.children().peekable();
        let mut children = cx.children();
        let mut occupied_cross = 0.0;
        let mut cross_cursor = cross_leading;
        let mut runs = 0;
        while targets.peek().is_some() {
            let run = next_run(
                &mut targets,
                |child| self.axis.extent(cx.target_size(*child)),
                target_available,
                item_gap,
                f32::EPSILON * target_available,
            );
            let count = run.count;
            let start = children;
            let mut used = item_gap * count.saturating_sub(1) as f32;
            let mut grows = 0;
            for child in start.take(count) {
                used += self.axis.extent(cx.size(child));
                if let Sizing::Grow { .. } = resolve_sizing(cx, child, self.axis, cx.item(child).sizing(self.axis)) {
                    grows += 1;
                }
            }
            let growth = if grows == 0 {
                0.0
            } else {
                (available - used).max(0.0) / grows as f32
            };
            let mut allocation = 0.0;
            let mut main = item_gap * count.saturating_sub(1) as f32;
            let mut cross: f32 = 0.0;
            for child in start.take(count) {
                let width = self.axis.extent(cx.size(child));
                let sizing = resolve_sizing(cx, child, self.axis, cx.item(child).sizing(self.axis));
                if let Sizing::Grow { .. } = sizing {
                    let assigned =
                        sizing.clamp(width + C::allocate(&mut allocation, sizing.clamp(width + growth) - width));
                    if assigned != width {
                        cx.layout_child(
                            child,
                            flow_constraints(
                                self.axis,
                                (assigned, assigned),
                                sizing_range::<C>(
                                    resolve_sizing(cx, child, cross_axis, cx.item(child).sizing(cross_axis)),
                                    cross_max,
                                ),
                            ),
                        );
                    }
                }
                let size = cx.size(child);
                main += self.axis.extent(size);
                cross = cross.max(cross_axis.extent(size));
            }
            let (offset, extra_gap) = justify_offset(self.justify, (available - main).max(0.0), count);
            let mut main_cursor = main_leading + offset;
            for child in children.by_ref().take(count) {
                let size = cx.size(child);
                let child_main = self.axis.extent(size);
                let child_cross = cross_axis.extent(size);
                let sizing = cx.item(child).sizing(cross_axis);
                let assigned = match (sizing, self.align) {
                    (Sizing::Grow { .. }, _) | (Sizing::Fit { .. }, Align::Stretch) => {
                        resolve_sizing(cx, child, cross_axis, sizing).clamp(cross)
                    }
                    _ => child_cross,
                };
                if assigned != child_cross {
                    cx.layout_child(child, Constraints::tight(flow_size(child_main, assigned, self.axis)));
                }
                let child_cross = cross_axis.extent(cx.size(child));
                let cross_offset = match self.align {
                    Align::Start | Align::Stretch => 0.0,
                    Align::Center => (cross - child_cross).max(0.0) / 2.0,
                    Align::End => (cross - child_cross).max(0.0),
                };
                let pos = flow_size(C::round(main_cursor), C::round(cross_cursor + cross_offset), self.axis);
                cx.set_child_position(child, Point::new(pos.width, pos.height));
                main_cursor += child_main + item_gap + extra_gap;
            }
            occupied_cross += cross + if runs == 0 { 0.0 } else { run_gap };
            cross_cursor += cross + run_gap;
            runs += 1;
        }
        bounds.constrain(flow_size(
            self.axis.extent(size),
            occupied_cross + cross_padding,
            self.axis,
        ))
    }
}

struct Run {
    count: usize,
    used: f32,
}

fn next_run<I: Iterator>(
    widths: &mut std::iter::Peekable<I>,
    mut size: impl FnMut(&I::Item) -> f32,
    available: f32,
    gap: f32,
    tolerance: f32,
) -> Run {
    let mut run = Run { count: 0, used: 0.0 };
    while let Some(item) = widths.peek() {
        let width = size(item);
        let needed = width + if run.count == 0 { 0.0 } else { gap };
        if run.count != 0 && run.used + needed - available > tolerance * (run.count + 1) as f32 {
            break;
        }
        widths.next();
        run.used += needed;
        run.count += 1;
    }
    run
}

fn longest_run(widths: impl Iterator<Item = f32>, available: f32, gap: f32) -> f32 {
    let mut widths = widths.peekable();
    let mut longest: f32 = 0.0;
    while widths.peek().is_some() {
        longest = longest.max(next_run(&mut widths, |width| *width, available, gap, 0.0).used);
    }
    longest
}
