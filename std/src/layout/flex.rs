use blit::{Axis, Constraints, Layout, LayoutCx, Platform, Point, Sides, Size, Sizing};

use super::{
    Align, Justify, flow_constraints, flow_size, justify_offset, override_sizing, size_on_axis,
    sizing_range,
};

blit::builder! {
    /// sizing policy for a flex child
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct FlexItem {
        new(),
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
    }
}

impl FlexItem {
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
    /// flex layout of a container's direct children
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Flex {
        new(axis: Axis),
        padding: Sides = Sides::all(0.0),
        gap: f32 = 0.0,
        align: Align = Align::Stretch,
        justify: Justify = Justify::Start,
        overflow: bool = false,
    }
}

impl Flex {
    pub fn row() -> Self {
        Self::new(Axis::Horizontal)
    }

    pub fn column() -> Self {
        Self::new(Axis::Vertical)
    }
}

impl<P: Platform> Layout<P> for Flex {
    type Item = FlexItem;

    fn size_override(&self, item: &mut Self::Item, width: Option<f32>, height: Option<f32>) {
        override_sizing(&mut item.width, &mut item.height, width, height);
    }

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, constraints: Constraints) -> Size {
        fn minimum(sizing: Sizing, resolved: f32) -> f32 {
            match sizing {
                Sizing::Grow { min, .. } => min.max(0.0),
                Sizing::Fit { .. } | Sizing::Fixed(_) | Sizing::Percent(_) => resolved,
            }
        }

        fn range(sizing: Sizing, available: f32, stretch: bool) -> (f32, f32) {
            if stretch {
                let size = match sizing {
                    Sizing::Percent(_) => sizing.resolve(0.0, available, true),
                    _ => sizing.clamp(available),
                };
                return (size, size);
            }
            if let Sizing::Fixed(size) = sizing {
                let size = size.max(0.0).min(available);
                return (size, size);
            }
            sizing_range(sizing, available)
        }

        let count = cx.children().count();
        if count == 0 {
            return constraints.constrain(Size::default());
        }

        let cross_axis = match self.axis {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        };
        let padding = cx.resolve_sides(self.padding);
        let gap = cx.resolve_extent(self.axis, self.gap).max(0.0);
        let gaps = gap * count.saturating_sub(1) as f32;
        let (main_padding, cross_padding) = match self.axis {
            Axis::Horizontal => (padding.left + padding.right, padding.top + padding.bottom),
            Axis::Vertical => (padding.top + padding.bottom, padding.left + padding.right),
        };
        let max_main = (size_on_axis(constraints.max, self.axis) - main_padding - gaps).max(0.0);
        let max_cross = (size_on_axis(constraints.max, cross_axis) - cross_padding).max(0.0);
        let tight_cross =
            size_on_axis(constraints.min, cross_axis) == size_on_axis(constraints.max, cross_axis);

        let mut used = 0.0;
        let mut natural_cross: f32 = 0.0;
        let mut grow = 0usize;
        for node in cx.children() {
            let item = cx.item(node);
            let main_sizing = cx.resolve_sizing(self.axis, item.sizing(self.axis));
            let cross_sizing = cx.resolve_sizing(cross_axis, item.sizing(cross_axis));
            let main_available = if self.overflow
                || matches!(
                    main_sizing,
                    Sizing::Fit { .. } | Sizing::Grow { .. } | Sizing::Percent(_)
                ) {
                f32::INFINITY
            } else {
                max_main
            };
            let cross_stretch = tight_cross
                && (matches!(cross_sizing, Sizing::Grow { .. })
                    || self.align == Align::Stretch && matches!(cross_sizing, Sizing::Fit { .. }));
            let constraints = flow_constraints(
                self.axis,
                range(main_sizing, main_available, false),
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
            let child = cx.layout_child(node, constraints);
            used += size_on_axis(child, self.axis);
            natural_cross = natural_cross.max(size_on_axis(child, cross_axis));
            grow += usize::from(matches!(main_sizing, Sizing::Grow { .. }));
        }
        let natural = flow_size(
            used + gaps + main_padding,
            natural_cross + cross_padding,
            self.axis,
        );
        let mut size = constraints.constrain(natural);
        let available_main = (size_on_axis(size, self.axis) - main_padding).max(0.0);
        let percentage_available = (available_main - gaps).max(0.0);
        let mut main_changed = false;
        for node in cx.children() {
            let sizing = cx.resolve_sizing(self.axis, cx.item(node).sizing(self.axis));
            if let Sizing::Percent(fraction) = sizing {
                let child_size =
                    Sizing::Percent(fraction).resolve(0.0, percentage_available, false);
                cx.set_size(node, self.axis, child_size);
                used += child_size;
                main_changed = true;
            }
        }
        let free = available_main - used - gaps;

        if free < 0.0 && !self.overflow {
            let mut capacity = 0.0;
            for node in cx.children() {
                let child_size = cx.axis_size(node, self.axis);
                let sizing = cx.resolve_sizing(self.axis, cx.item(node).sizing(self.axis));
                capacity += child_size - minimum(sizing, child_size);
            }
            if capacity > 0.0 {
                let deficit = (-free).min(capacity);
                for node in cx.children() {
                    let child_size = cx.axis_size(node, self.axis);
                    let available_shrink = child_size
                        - minimum(
                            cx.resolve_sizing(self.axis, cx.item(node).sizing(self.axis)),
                            child_size,
                        );
                    let shrunk = child_size - deficit * available_shrink / capacity;
                    cx.set_size(node, self.axis, shrunk);
                    used += shrunk - child_size;
                    main_changed |= shrunk != child_size;
                }
            }
        }

        let mut free = (available_main - used - gaps).max(0.0);
        let mut remaining_grow = grow;
        while remaining_grow != 0 && free > 0.0 {
            let share = free / remaining_grow as f32;
            let mut distributed = 0.0;
            let mut uncapped = 0;
            for node in cx.children() {
                let sizing = cx.resolve_sizing(self.axis, cx.item(node).sizing(self.axis));
                if matches!(sizing, Sizing::Grow { .. }) {
                    let child_size = cx.axis_size(node, self.axis);
                    let maximum = sizing.clamp(f32::INFINITY);
                    if maximum <= child_size {
                        continue;
                    }
                    let grown = sizing.clamp(child_size + share);
                    cx.set_size(node, self.axis, grown);
                    distributed += grown - child_size;
                    main_changed |= grown != child_size;
                    uncapped += usize::from(maximum > grown);
                }
            }
            used += distributed;
            free = (free - distributed).max(0.0);
            if distributed == 0.0 || uncapped == remaining_grow {
                break;
            }
            remaining_grow = uncapped;
        }

        let available_cross = (size_on_axis(size, cross_axis) - cross_padding).max(0.0);
        for node in cx.children() {
            let main_size = cx.axis_size(node, self.axis);
            let item = cx.item(node);
            let main_sizing = cx.resolve_sizing(self.axis, item.sizing(self.axis));
            let cross_sizing = cx.resolve_sizing(cross_axis, item.sizing(cross_axis));
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
            let cross_changed = cross.0 == cross.1 && cross.0 != cx.axis_size(node, cross_axis);
            if (main_changed && !matches!(main_sizing, Sizing::Fixed(_))) || cross_changed {
                cx.constrain_child(
                    node,
                    flow_constraints(self.axis, (main_size, main_size), cross),
                );
            }
        }

        natural_cross = 0.0;
        for node in cx.children() {
            natural_cross = natural_cross.max(cx.axis_size(node, cross_axis));
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

        for node in cx.children() {
            let cross_sizing = cx.resolve_sizing(cross_axis, cx.item(node).sizing(cross_axis));
            let stretch = matches!(cross_sizing, Sizing::Grow { .. })
                || self.align == Align::Stretch && matches!(cross_sizing, Sizing::Fit { .. });
            if stretch && cx.axis_size(node, cross_axis) != cross_sizing.clamp(available_cross) {
                let main_size = cx.axis_size(node, self.axis);
                let cross_size = cross_sizing.clamp(available_cross);
                cx.constrain_child(
                    node,
                    flow_constraints(self.axis, (main_size, main_size), (cross_size, cross_size)),
                );
            }
        }

        let remaining = (available_main - used - gaps).max(0.0);
        let (offset, extra_gap) = justify_offset(self.justify, remaining, count);
        let (main_leading, cross_leading) = match self.axis {
            Axis::Horizontal => (padding.left, padding.top),
            Axis::Vertical => (padding.top, padding.left),
        };
        let mut cursor = main_leading + offset;
        for node in cx.children() {
            let main_size = cx.axis_size(node, self.axis);
            let cross_size = cx.axis_size(node, cross_axis);
            let cross_offset = match self.align {
                Align::Start | Align::Stretch => 0.0,
                Align::Center => (available_cross - cross_size).max(0.0) / 2.0,
                Align::End => (available_cross - cross_size).max(0.0),
            };
            let position = flow_size(cursor, cross_leading + cross_offset, self.axis);
            cx.set_position(
                node,
                Point {
                    x: position.width,
                    y: position.height,
                },
            );
            cursor += main_size + gap + extra_gap;
        }

        size
    }
}
