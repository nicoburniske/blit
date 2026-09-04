use blit::{Axis, Constraints, LayoutCx, Platform, Point, Sides, Size, Sizing};

pub use super::sizing::{Item, item};
use super::{
    Align, Justify, capped_growth, flow_constraints, flow_size, justify_offset, override_sizing,
    percentage, size_on_axis, sizing_range,
};

blit::builder! {
    /// flex layout of a container's direct children
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Layout {
        new(axis: Axis),
        padding: Sides = Sides::all(0.0),
        gap: f32 = 0.0,
        align: Align = Align::Stretch,
        justify: Justify = Justify::Start,
        overflow: bool = false,
    }
}

pub fn layout(axis: Axis) -> Layout {
    Layout::new(axis)
}

pub fn row() -> Layout {
    layout(Axis::Horizontal)
}

pub fn column() -> Layout {
    layout(Axis::Vertical)
}

impl<P: Platform> blit::Layout<P> for Layout {
    type Item = Item;

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, constraints: Constraints) -> Size {
        fn range(sizing: Sizing, available: f32, stretch: bool) -> (f32, f32) {
            if stretch {
                let size = match sizing {
                    Sizing::Percent(fraction) => percentage(fraction, available),
                    _ => sizing.clamp(available),
                };
                return (size, size);
            }
            sizing_range(sizing, available)
        }

        fn allocated(
            sizing: Sizing,
            natural: f32,
            available: f32,
            shrink: f32,
            growth: f32,
        ) -> f32 {
            let base = match sizing {
                Sizing::Percent(fraction) => percentage(fraction, available),
                _ => natural,
            };
            match sizing {
                Sizing::Grow { min, .. } if shrink > 0.0 => base - (base - min.max(0.0)) * shrink,
                Sizing::Grow { .. } => {
                    base + growth.min((sizing.clamp(f32::INFINITY) - base).max(0.0))
                }
                _ => base,
            }
        }

        let res = cx.resolution();
        let padding = res.sides(self.padding);
        let cross_axis = match self.axis {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        };
        let gap = res.extent(self.axis, self.gap).max(0.0);
        let (main_padding, cross_padding) = match self.axis {
            Axis::Horizontal => (padding.left + padding.right, padding.top + padding.bottom),
            Axis::Vertical => (padding.top + padding.bottom, padding.left + padding.right),
        };
        let max_cross = (size_on_axis(constraints.max, cross_axis) - cross_padding).max(0.0);
        let tight_cross =
            size_on_axis(constraints.min, cross_axis) == size_on_axis(constraints.max, cross_axis);

        let mut natural_main = 0.0;
        let mut natural_cross: f32 = 0.0;
        let mut count = 0usize;
        for child in cx.children() {
            count += 1;
            let item = cx.item(child);
            let main_sizing = res.sizing(self.axis, item.sizing(self.axis));
            let cross_sizing = res.sizing(cross_axis, item.sizing(cross_axis));
            let cross_stretch = tight_cross
                && (matches!(cross_sizing, Sizing::Grow { .. })
                    || self.align == Align::Stretch && matches!(cross_sizing, Sizing::Fit { .. }));
            let constraints = flow_constraints(
                self.axis,
                range(main_sizing, f32::INFINITY, false),
                range(
                    cross_sizing,
                    if matches!(cross_sizing, Sizing::Percent(_)) && !cross_stretch {
                        f32::INFINITY
                    } else {
                        max_cross
                    },
                    cross_stretch,
                ),
            );
            let child_size = cx.layout_child(child, constraints);
            natural_main += size_on_axis(child_size, self.axis);
            natural_cross = natural_cross.max(size_on_axis(child_size, cross_axis));
        }
        if count == 0 {
            return constraints.constrain(padding.size());
        }
        let gaps = gap * count.saturating_sub(1) as f32;
        let natural = flow_size(
            natural_main + gaps + main_padding,
            natural_cross + cross_padding,
            self.axis,
        );
        let mut size = constraints.constrain(natural);
        let available_main = (size_on_axis(size, self.axis) - main_padding).max(0.0);
        let percentage_available = (available_main - gaps).max(0.0);
        let mut used = 0.0;
        let mut shrink_capacity = 0.0;
        let mut grow = 0usize;
        let mut minimum_growth = f32::INFINITY;
        for child in cx.children() {
            let sizing = res.sizing(self.axis, cx.item(child).sizing(self.axis));
            let natural = size_on_axis(cx.child_size(child), self.axis);
            let allocated = allocated(sizing, natural, percentage_available, 0.0, 0.0);
            used += allocated;
            if let Sizing::Grow { min, .. } = sizing {
                shrink_capacity += natural - min.max(0.0);
                let capacity = (sizing.clamp(f32::INFINITY) - natural).max(0.0);
                if capacity > 0.0 {
                    grow += 1;
                    minimum_growth = minimum_growth.min(capacity);
                }
            }
        }
        let free = available_main - used - gaps;

        let mut shrink = 0.0;
        if free < 0.0 && !self.overflow && shrink_capacity > 0.0 {
            let deficit = (-free).min(shrink_capacity);
            shrink = deficit / shrink_capacity;
            used -= deficit;
        }

        let free = (available_main - used - gaps).max(0.0);
        let growth = capped_growth(
            free,
            grow,
            minimum_growth,
            cx.children().filter_map(|child| {
                let sizing = res.sizing(self.axis, cx.item(child).sizing(self.axis));
                matches!(sizing, Sizing::Grow { .. }).then(|| {
                    let natural = size_on_axis(cx.child_size(child), self.axis);
                    (sizing.clamp(f32::INFINITY) - natural).max(0.0)
                })
            }),
        );
        let available_cross = (size_on_axis(size, cross_axis) - cross_padding).max(0.0);
        used = 0.0;
        natural_cross = 0.0;
        for child in cx.children() {
            let natural_main = size_on_axis(cx.child_size(child), self.axis);
            let item = cx.item(child);
            let main_sizing = res.sizing(self.axis, item.sizing(self.axis));
            let cross_sizing = res.sizing(cross_axis, item.sizing(cross_axis));
            let main_size = allocated(
                main_sizing,
                natural_main,
                percentage_available,
                shrink,
                growth,
            );
            used += main_size;
            let stretch = tight_cross
                && (matches!(cross_sizing, Sizing::Grow { .. })
                    || self.align == Align::Stretch && matches!(cross_sizing, Sizing::Fit { .. }));
            let cross = range(
                cross_sizing,
                if tight_cross {
                    available_cross
                } else {
                    max_cross
                },
                stretch,
            );
            let cross_changed =
                cross.0 == cross.1 && cross.0 != size_on_axis(cx.child_size(child), cross_axis);
            if main_size != natural_main || cross_changed {
                cx.layout_child(
                    child,
                    flow_constraints(self.axis, (main_size, main_size), cross),
                );
            }
            natural_cross = natural_cross.max(size_on_axis(cx.child_size(child), cross_axis));
        }
        let resolved_cross = size_on_axis(
            constraints.constrain(flow_size(
                size_on_axis(size, self.axis),
                natural_cross + cross_padding,
                self.axis,
            )),
            cross_axis,
        );
        match cross_axis {
            Axis::Horizontal => size.width = resolved_cross,
            Axis::Vertical => size.height = resolved_cross,
        }
        let available_cross = (resolved_cross - cross_padding).max(0.0);

        let remaining = (available_main - used - gaps).max(0.0);
        let (offset, extra_gap) = justify_offset(self.justify, remaining, count);
        let (main_leading, cross_leading) = match self.axis {
            Axis::Horizontal => (padding.left, padding.top),
            Axis::Vertical => (padding.top, padding.left),
        };
        let mut cursor = main_leading + offset;
        for child in cx.children() {
            let cross_sizing = res.sizing(cross_axis, cx.item(child).sizing(cross_axis));
            let assigned_cross = match cross_sizing {
                Sizing::Percent(fraction) => Some(percentage(fraction, available_cross)),
                Sizing::Grow { .. } => Some(cross_sizing.clamp(available_cross)),
                Sizing::Fit { .. } if self.align == Align::Stretch => {
                    Some(cross_sizing.clamp(available_cross))
                }
                _ => None,
            };
            let mut child_size = cx.child_size(child);
            let main_size = size_on_axis(child_size, self.axis);
            if let Some(assigned_cross) = assigned_cross
                && size_on_axis(child_size, cross_axis) != assigned_cross
            {
                child_size = cx.layout_child(
                    child,
                    flow_constraints(
                        self.axis,
                        (main_size, main_size),
                        (assigned_cross, assigned_cross),
                    ),
                );
            }
            let cross_size = size_on_axis(child_size, cross_axis);
            let cross_offset = match self.align {
                Align::Start | Align::Stretch => 0.0,
                Align::Center => (available_cross - cross_size).max(0.0) / 2.0,
                Align::End => (available_cross - cross_size).max(0.0),
            };
            let position = flow_size(cursor, cross_leading + cross_offset, self.axis);
            cx.set_child_position(child, Point::new(position.width, position.height));
            cursor += main_size + gap + extra_gap;
        }

        size
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
