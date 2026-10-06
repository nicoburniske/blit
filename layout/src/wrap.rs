use blit::{Axis, Constraints, Context, LayoutCx, Point, Scalar, Sides, Size};

use super::{
    Align, Justify, Sizing, allocated_range, flow_constraints, flow_size, justify_offset, resolve_sizing, sizing_range,
};

blit::builder! {
    /// wraps children into rows or columns
    ///
    /// measured sizes choose the runs, then grow children share the space left in each run
    /// if a grow child reaches its limit, the unused space is left for justification
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout<T: Scalar> {
        new(axis: Axis),
        padding: Sides<T> = Sides::all(T::ZERO),
        item_gap: T = T::ZERO,
        run_gap: T = T::ZERO,
        align: Align = Align::Start,
        justify: Justify = Justify::Start,
    }
}

blit::builder! {
    /// sizing policy for a wrapping child
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Item<T: Scalar> {
        new(),
        width: Sizing<T> = Sizing::fit(),
        height: Sizing<T> = Sizing::fit(),
    }
}

impl<T: Scalar> Layout<T> {
    pub fn gap(mut self, gap: T) -> Self {
        self.item_gap = gap;
        self.run_gap = gap;
        self
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

pub fn new<T: Scalar>(axis: Axis) -> Layout<T> {
    Layout::new(axis)
}

pub fn horizontal<T: Scalar>() -> Layout<T> {
    new(Axis::Horizontal)
}

pub fn vertical<T: Scalar>() -> Layout<T> {
    new(Axis::Vertical)
}

pub fn item<T: Scalar>() -> Item<T> {
    Item::new()
}

impl<C: Context<Scalar = T>, T: Scalar> blit::Layout<C> for Layout<T> {
    type Item = Item<T>;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints<T>) -> Size<T> {
        let padding = self.padding;
        let cross_axis = self.axis.other();
        let main_padding = self.axis.extent(padding.size());
        let cross_padding = cross_axis.extent(padding.size());
        let leading = Size::new(padding.left, padding.top);
        let main_leading = self.axis.extent(leading);
        let item_gap = self.item_gap.max(T::ZERO);
        let run_gap = self.run_gap.max(T::ZERO);
        let inner_bounds = bounds.shrink(padding.size());
        let main_min = self.axis.extent(inner_bounds.min);
        let main_max = self.axis.extent(inner_bounds.max);
        let cross_max = cross_axis.extent(inner_bounds.max);
        let mut percentages = 0.0;
        let mut target_differs = false;
        for child in cx.children() {
            let item = cx.item(child);
            let main = resolve_sizing(cx, child, self.axis, item.sizing(self.axis));
            let size = cx.layout_child(
                child,
                flow_constraints(
                    self.axis,
                    allocated_range(main, main_max, &mut percentages),
                    sizing_range(
                        resolve_sizing(cx, child, cross_axis, item.sizing(cross_axis)),
                        cross_max,
                    ),
                ),
            );
            target_differs |= self.axis.extent(size) != self.axis.extent(cx.target_size(child));
        }
        let longest = longest_run(
            cx.children().map(|child| self.axis.extent(cx.size(child))),
            main_max,
            item_gap,
        );
        let available = longest.clamp(main_min, main_max);
        let target_available = if target_differs {
            longest_run(
                cx.children().map(|child| self.axis.extent(cx.target_size(child))),
                main_max,
                item_gap,
            )
            .clamp(main_min, main_max)
        } else {
            available
        };
        let mut targets = cx.children().peekable();
        let mut children = cx.children();
        let mut occupied_cross = T::ZERO;
        let mut cross_cursor = cross_axis.extent(leading);
        let mut runs = 0;
        while targets.peek().is_some() {
            let (count, _) = next_run(
                &mut targets,
                |child| self.axis.extent(cx.target_size(*child)),
                target_available,
                item_gap,
            );
            let gaps = item_gap.repeat(count.saturating_sub(1));
            let mut used = gaps;
            let mut grows = 0;
            for child in children.take(count) {
                used += self.axis.extent(cx.size(child));
                if let Sizing::Grow { .. } = resolve_sizing(cx, child, self.axis, cx.item(child).sizing(self.axis)) {
                    grows += 1;
                }
            }
            let growth = if grows == 0 {
                0.0
            } else {
                (available - used).max(T::ZERO).to_f32() / grows as f32
            };
            let mut allocation = 0.0;
            let mut main = gaps;
            let mut cross = T::ZERO;
            for child in children.take(count) {
                let width = self.axis.extent(cx.size(child));
                let sizing = resolve_sizing(cx, child, self.axis, cx.item(child).sizing(self.axis));
                if let Sizing::Grow { min, max } = sizing {
                    let capacity = if max == T::UNBOUNDED {
                        f32::INFINITY
                    } else {
                        (max.max(min.max(T::ZERO)) - width).max(T::ZERO).to_f32()
                    };
                    let assigned = sizing.clamp(width + T::allocate(&mut allocation, growth.min(capacity)));
                    if assigned != width {
                        cx.layout_child(
                            child,
                            flow_constraints(
                                self.axis,
                                (assigned, assigned),
                                sizing_range(
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
            let (offset, mut spacing, extra_gap) = justify_offset(self.justify, (available - main).max(T::ZERO), count);
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
                    Align::Start | Align::Stretch => T::ZERO,
                    Align::Center => T::ZERO.lerp((cross - child_cross).max(T::ZERO), 0.5),
                    Align::End => (cross - child_cross).max(T::ZERO),
                };
                let pos = flow_size(
                    main_cursor + T::from_f32(spacing),
                    cross_cursor + cross_offset,
                    self.axis,
                );
                cx.set_child_position(child, Point::new(pos.width, pos.height));
                main_cursor += child_main + item_gap;
                spacing += extra_gap;
            }
            occupied_cross += cross + if runs == 0 { T::ZERO } else { run_gap };
            cross_cursor += cross + run_gap;
            runs += 1;
        }
        bounds.constrain(flow_size(
            available + main_padding,
            occupied_cross + cross_padding,
            self.axis,
        ))
    }
}

fn next_run<T: Scalar, I: Iterator>(
    widths: &mut std::iter::Peekable<I>,
    mut size: impl FnMut(&I::Item) -> T,
    available: T,
    gap: T,
) -> (usize, T) {
    let mut count = 0;
    let mut used = T::ZERO;
    let tolerance = available.tolerance();
    while let Some(item) = widths.peek() {
        let width = size(item);
        let needed = width.endpoint(if count == 0 { T::ZERO } else { gap });
        let total = used.endpoint(needed);
        if count != 0 && total > available.endpoint(tolerance.repeat(count + 1)) {
            break;
        }
        widths.next();
        used = total;
        count += 1;
    }
    (count, used)
}

fn longest_run<T: Scalar>(widths: impl Iterator<Item = T>, available: T, gap: T) -> T {
    let mut widths = widths.peekable();
    let mut longest = T::ZERO;
    while widths.peek().is_some() {
        longest = longest.max(next_run(&mut widths, |width| *width, available, gap).1);
    }
    longest
}
