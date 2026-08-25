//! user-defined frame layout policies and child declaration scopes
//!
//! layout resolves the complete frame graph in ordered passes:
//!
//! 1. measure intrinsic content
//! 2. measure bottom-up and place top-down horizontally
//! 3. wrap text at its resolved width
//! 4. measure bottom-up and place top-down vertically
//!
//! [`Layout`] operates on one [`Axis`] at a time. this staged model is not
//! recursive constraint propagation. layouts can place any widget subtree with
//! arbitrary axis-aligned geometry, including overlap and radial arrangements.
//! layouts may set sibling paint order with [`LayoutCx::set_z_index`]. positions
//! must remain relative to [`LayoutCx::rect`] so moving a container only
//! translates its descendants.

mod flex;

pub use crate::graph::{Children, LayoutCx};
pub use flex::*;

use crate::{Ui, node::NodeId, widget::Widget};

/// layout policy for a container's direct children
///
/// moving a container must only translate its descendants
pub trait Layout: Sized + 'static {
    /// per-child layout metadata
    ///
    /// `()` can use [`UnitScope`] to avoid per-child metadata bookkeeping
    /// layouts using `UnitScope` do not call [`LayoutCx::item`]
    type Item: Copy + 'static;

    /// child declaration scope
    type Scope<'a>: From<RawScope<'a, Self>>;

    /// intrinsic size on one axis, or `None` to preserve core's size
    fn measure(&self, cx: &LayoutCx<'_, Self::Item>, axis: Axis) -> Option<f32>;

    /// places children on one axis
    fn place(&self, cx: &mut LayoutCx<'_, Self::Item>, axis: Axis);
}

/// low-level child declaration scope for custom layout scopes
pub struct RawScope<'ui, L: Layout> {
    ui: &'ui mut Ui,
    node: NodeId,
    layout: std::marker::PhantomData<fn() -> L>,
}

/// scope for layouts whose children need no metadata
pub struct UnitScope<'ui, L: Layout<Item = ()> = Flex>(RawScope<'ui, L>);

/// scope for layouts whose children require metadata
pub struct ItemScope<'ui, L: Layout>(RawScope<'ui, L>);

impl<L: Layout> RawScope<'_, L> {
    /// declares one child subtree and attaches its layout metadata
    pub fn add<W: Widget>(&mut self, item: L::Item, widget: W) -> W::Output {
        let child = self.ui.begin_layout_item();
        let output = widget.render(self.ui);
        self.ui.finish_layout_item::<L>(self.node, child, item);
        output
    }
}

impl<'ui, L: Layout<Item = ()>> From<RawScope<'ui, L>> for UnitScope<'ui, L> {
    fn from(scope: RawScope<'ui, L>) -> Self {
        Self(scope)
    }
}

impl<L: Layout<Item = ()>> UnitScope<'_, L> {
    /// renders one child subtree without storing layout metadata
    ///
    /// unit carries no information, so layouts using this scope do not call
    /// [`LayoutCx::item`]
    pub fn add<W: Widget>(&mut self, widget: W) -> W::Output {
        widget.render(self.0.ui)
    }
}

impl<'ui, L: Layout> From<RawScope<'ui, L>> for ItemScope<'ui, L> {
    fn from(scope: RawScope<'ui, L>) -> Self {
        Self(scope)
    }
}

impl<L: Layout> ItemScope<'_, L> {
    pub fn add<W: Widget>(&mut self, item: L::Item, widget: W) -> W::Output {
        self.0.add(item, widget)
    }
}

impl<L: Layout> Drop for RawScope<'_, L> {
    fn drop(&mut self) {
        self.ui.close_container(self.node)
    }
}

/// layout axis
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    #[default]
    Vertical,
}

pub(crate) fn raw_scope<L: Layout>(ui: &mut Ui, node: NodeId) -> RawScope<'_, L> {
    RawScope {
        ui,
        node,
        layout: std::marker::PhantomData,
    }
}
