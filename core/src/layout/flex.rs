use crate::{container::Sizing, geometry::LogicalInsets};

use super::{Axis, Layout, LayoutCx, UnitScope};

crate::builder! {
    /// flex layout of a container's direct children
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Flex {
        new(axis: Axis),
        padding: LogicalInsets = LogicalInsets::uniform(0.0),
        gap: f32 = 0.0,
        align: Align = Align::Stretch,
        justify: Justify = Justify::Start,
        overflow: bool = false,
    }
}

/// child alignment across the flow axis
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

/// child distribution along the flow axis
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl Flex {
    pub fn row() -> Self {
        Self::new(Axis::Horizontal)
    }

    pub fn column() -> Self {
        Self::new(Axis::Vertical)
    }
}

impl Layout for Flex {
    type Item = ();
    type Scope<'a> = UnitScope<'a, Self>;

    fn measure(&self, cx: &LayoutCx<'_, Self::Item>, axis: Axis) -> Option<f32> {
        let along_flow = self.axis == axis;
        let mut measured: f32 = 0.0;
        let mut count = 0usize;
        for node in cx.children() {
            if cx.is_in_flow(node) {
                let sizing = cx.sizing(node, axis);
                let size = sizing.resolve(cx.axis_size(node, axis), f32::INFINITY, !along_flow);
                measured = if along_flow {
                    measured + size
                } else {
                    measured.max(size)
                };
                count += 1;
            }
        }
        if count == 0 {
            return None;
        }
        if along_flow {
            measured += self.gap.max(0.0) * count.saturating_sub(1) as f32;
        }
        measured += match axis {
            Axis::Horizontal => self.padding.left + self.padding.right,
            Axis::Vertical => self.padding.top + self.padding.bottom,
        };
        Some(measured)
    }

    fn place(&self, cx: &mut LayoutCx<'_, Self::Item>, axis: Axis) {
        let parent_area = cx.rect();
        let (origin, available, flow, leading, trailing) = match axis {
            Axis::Horizontal => (
                parent_area.x,
                parent_area.width,
                self.axis == Axis::Horizontal,
                self.padding.left,
                self.padding.right,
            ),
            Axis::Vertical => (
                parent_area.y,
                parent_area.height,
                self.axis == Axis::Vertical,
                self.padding.top,
                self.padding.bottom,
            ),
        };
        let available = (available - leading - trailing).max(0.0);

        if !flow {
            for node in cx.children() {
                if cx.is_in_flow(node) {
                    let sizing = cx.sizing(node, axis);
                    let mut size = sizing.resolve(cx.axis_size(node, axis), available, true);
                    if self.align == Align::Stretch && matches!(sizing, Sizing::Fit { .. }) {
                        size = sizing.clamp(available);
                    }
                    let offset = match self.align {
                        Align::Start | Align::Stretch => 0.0,
                        Align::Center => (available - size).max(0.0) / 2.0,
                        Align::End => (available - size).max(0.0),
                    };
                    cx.set_axis(node, axis, origin + leading + offset, size);
                }
            }
            return;
        }

        let mut count = 0usize;
        let mut grow = 0usize;
        let mut has_percentage = false;
        let mut used = 0.0;
        for node in cx.children() {
            if cx.is_in_flow(node) {
                let sizing = cx.sizing(node, axis);
                let percentage = matches!(sizing, Sizing::Percent(_));
                has_percentage |= percentage;
                let size = sizing.resolve(
                    cx.axis_size(node, axis),
                    if self.overflow || percentage {
                        f32::INFINITY
                    } else {
                        available
                    },
                    false,
                );
                cx.set_size(node, axis, size);
                used += size;
                count += 1;
                grow += usize::from(matches!(sizing, Sizing::Grow { .. }));
            }
        }
        let gaps = self.gap.max(0.0) * count.saturating_sub(1) as f32;
        if has_percentage {
            let percentage_available = (available - gaps).max(0.0);
            for node in cx.children() {
                if cx.is_in_flow(node)
                    && let Sizing::Percent(fraction) = cx.sizing(node, axis)
                {
                    let size = Sizing::Percent(fraction).resolve(0.0, percentage_available, false);
                    cx.set_size(node, axis, size);
                    used += size;
                }
            }
        }
        let free = available - used - gaps;
        if free < 0.0 && !self.overflow {
            let mut capacity = 0.0;
            for node in cx.children() {
                if cx.is_in_flow(node) {
                    let size = cx.axis_size(node, axis);
                    capacity += size - cx.sizing(node, axis).minimum(size);
                }
            }
            if capacity > 0.0 {
                let deficit = (-free).min(capacity);
                for node in cx.children() {
                    if cx.is_in_flow(node) {
                        let size = cx.axis_size(node, axis);
                        let available_shrink = size - cx.sizing(node, axis).minimum(size);
                        let shrunk = size - deficit * available_shrink / capacity;
                        cx.set_size(node, axis, shrunk);
                        used += shrunk - size;
                    }
                }
            }
        }
        let free = free.max(0.0);
        if grow != 0 {
            let share = free / grow as f32;
            for node in cx.children() {
                if cx.is_in_flow(node) && matches!(cx.sizing(node, axis), Sizing::Grow { .. }) {
                    let sizing = cx.sizing(node, axis);
                    let size = cx.axis_size(node, axis);
                    let grown = sizing.clamp(size + share);
                    cx.set_size(node, axis, grown);
                    used += grown - size;
                }
            }
        }

        let remaining = (available - used - gaps).max(0.0);
        let (offset, extra_gap) = match self.justify {
            Justify::Start => (0.0, 0.0),
            Justify::Center => (remaining / 2.0, 0.0),
            Justify::End => (remaining, 0.0),
            Justify::SpaceBetween if count > 1 => (0.0, remaining / (count - 1) as f32),
            Justify::SpaceAround if count != 0 => {
                let space = remaining / count as f32;
                (space / 2.0, space)
            }
            Justify::SpaceEvenly if count != 0 => {
                let space = remaining / (count + 1) as f32;
                (space, space)
            }
            _ => (0.0, 0.0),
        };
        let mut cursor = origin + leading + offset;
        for node in cx.children() {
            if cx.is_in_flow(node) {
                let size = cx.axis_size(node, axis);
                cx.set_axis(node, axis, cursor, size);
                cursor += size + self.gap.max(0.0) + extra_gap;
            }
        }
    }
}
