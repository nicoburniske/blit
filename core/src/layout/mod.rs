//! user-defined frame layout policies and child declaration scopes
//!
//! layout passes constraints down the tree and returns sizes upward, then
//! resolves parent-local positions to absolute coordinates.

mod flex;
mod grid;
mod rect;
mod wrap;

pub use crate::frame::{Children, LayoutCx};
pub use flex::*;
pub use grid::*;
pub use rect::*;
pub use wrap::*;

use crate::{
    Ui,
    container::{LayerId, Sizing},
    geometry::LogicalSize,
    node::NodeId,
    widget::Widget,
};

/// layout policy for a container's direct children
///
/// layout and item values are stored in an 8-byte-aligned arena
pub trait Layout: Copy + 'static {
    /// per-child layout metadata
    ///
    /// `()` can use [`UnitScope`] to avoid per-child metadata bookkeeping
    /// layouts using `UnitScope` do not call [`LayoutCx::item`]
    type Item: Copy + 'static;

    /// child declaration scope
    type Scope<'a>: From<RawScope<'a, Self>>;

    /// lays out direct children and returns this container's constrained size
    fn layout(&self, cx: &mut LayoutCx<'_, Self::Item>, constraints: Constraints) -> LogicalSize;
}

/// minimum and maximum dimensions available to a layout node
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints {
    pub min: LogicalSize,
    pub max: LogicalSize,
}

impl Constraints {
    pub const fn loose(max: LogicalSize) -> Self {
        Self {
            min: LogicalSize {
                width: 0.0,
                height: 0.0,
            },
            max,
        }
    }

    pub const fn tight(size: LogicalSize) -> Self {
        Self {
            min: size,
            max: size,
        }
    }

    pub fn constrain(self, size: LogicalSize) -> LogicalSize {
        LogicalSize {
            width: size.width.clamp(self.min.width, self.max.width),
            height: size.height.clamp(self.min.height, self.max.height),
        }
    }
}

/// layout axis
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    #[default]
    Vertical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LayoutResolution {
    #[default]
    Continuous,
    Discrete {
        step: LogicalSize,
    },
}

impl LayoutResolution {
    #[inline]
    pub fn extent(self, axis: Axis, value: f32) -> f32 {
        let Self::Discrete { step } = self else {
            return value;
        };
        if value <= 0.0 || !value.is_finite() {
            return value;
        }
        let step = size_on_axis(step, axis);
        assert!(step.is_finite() && step > 0.0);
        (value / step).ceil() * step
    }

    #[inline(always)]
    pub fn sizing(self, axis: Axis, sizing: Sizing) -> Sizing {
        match sizing {
            Sizing::Fit { min, max } => Sizing::Fit {
                min: self.extent(axis, min),
                max: self.extent(axis, max),
            },
            Sizing::Grow { min, max } => Sizing::Grow {
                min: self.extent(axis, min),
                max: self.extent(axis, max),
            },
            Sizing::Fixed(size) => Sizing::Fixed(self.extent(axis, size)),
            Sizing::Percent(fraction) => Sizing::Percent(fraction),
        }
    }
}

/// child alignment across a layout's flow axis
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

/// child distribution along a layout's flow axis
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

/// scope for layouts whose children need no metadata
pub struct UnitScope<'ui, L: Layout<Item = ()> = Flex>(RawScope<'ui, L>);

impl<'ui, L: Layout<Item = ()>> From<RawScope<'ui, L>> for UnitScope<'ui, L> {
    fn from(scope: RawScope<'ui, L>) -> Self {
        Self(scope)
    }
}

impl<L: Layout<Item = ()>> UnitScope<'_, L> {
    pub fn node(&self) -> NodeId {
        self.0.node
    }

    pub fn layer(&mut self) -> LayerId {
        self.0.ui.layer()
    }

    /// renders one child subtree without storing layout metadata
    ///
    /// unit carries no information, so layouts using this scope do not call
    /// [`LayoutCx::item`]
    pub fn add<W: Widget>(&mut self, widget: W) -> W::Output {
        widget.render(self.0.ui)
    }

    pub fn close(self) {}
}

/// scope for layouts whose children require metadata
pub struct ItemScope<'ui, L: Layout>(RawScope<'ui, L>);

impl<'ui, L: Layout> From<RawScope<'ui, L>> for ItemScope<'ui, L> {
    fn from(scope: RawScope<'ui, L>) -> Self {
        Self(scope)
    }
}

impl<L: Layout> ItemScope<'_, L> {
    pub fn node(&self) -> NodeId {
        self.0.node
    }

    pub fn layer(&mut self) -> LayerId {
        self.0.ui.layer()
    }

    pub fn add<W: Widget>(&mut self, item: L::Item, widget: W) -> W::Output {
        self.0.add(item, widget)
    }

    pub fn close(self) {}
}

/// low-level child declaration scope for custom layout scopes
#[non_exhaustive]
pub struct RawScope<'ui, L: Layout> {
    pub ui: &'ui mut Ui,
    pub node: NodeId,
    pub layout: L,
}

impl<L: Layout> RawScope<'_, L> {
    /// declares one child subtree and attaches its layout metadata
    pub fn add<W: Widget>(&mut self, item: L::Item, widget: W) -> W::Output {
        let child = self.ui.begin_layout_item();
        let output = widget.render(self.ui);
        self.ui.finish_layout_item::<L>(self.node, child, item);
        output
    }
}

impl<L: Layout> Drop for RawScope<'_, L> {
    fn drop(&mut self) {
        self.ui.close_container(self.node)
    }
}

// internals

fn size_on_axis(size: LogicalSize, axis: Axis) -> f32 {
    match axis {
        Axis::Horizontal => size.width,
        Axis::Vertical => size.height,
    }
}

fn flow_size(main: f32, cross: f32, axis: Axis) -> LogicalSize {
    match axis {
        Axis::Horizontal => LogicalSize {
            width: main,
            height: cross,
        },
        Axis::Vertical => LogicalSize {
            width: cross,
            height: main,
        },
    }
}

fn sizing_range(sizing: Sizing, available: f32) -> (f32, f32) {
    match sizing {
        Sizing::Fit { min, max } | Sizing::Grow { min, max } => {
            let min = min.max(0.0);
            (min, max.max(min).min(available).max(min))
        }
        Sizing::Fixed(size) => {
            let size = size.max(0.0);
            (size, size)
        }
        Sizing::Percent(_) => {
            let size = sizing.resolve(0.0, available, false);
            (size, size)
        }
    }
}

fn flow_constraints(axis: Axis, main: (f32, f32), cross: (f32, f32)) -> Constraints {
    let (width, height) = match axis {
        Axis::Horizontal => (main, cross),
        Axis::Vertical => (cross, main),
    };
    Constraints {
        min: LogicalSize {
            width: width.0,
            height: height.0,
        },
        max: LogicalSize {
            width: width.1,
            height: height.1,
        },
    }
}

fn justify_offset(justify: Justify, remaining: f32, count: usize) -> (f32, f32) {
    match justify {
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
    }
}
