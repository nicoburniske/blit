use std::{marker::PhantomData, num::NonZeroU16};

use super::{NodeId, Ui};
use crate::{
    Clip, Leaf, Platform, Widget,
    animation::Transition,
    geometry::{Point, Sides},
    interact::WidgetId,
    layout::Layout,
};

/// frame-local paint layer
///
/// do not store this across renders
#[cfg_attr(not(debug_assertions), repr(transparent))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerId(NonZeroU16, #[cfg(debug_assertions)] u16);

crate::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Slot {
        new(),
        @optional {
            layer: LayerId,
        },
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
        z_index: i16 = 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    Fit { min: f32, max: f32 },
    Grow { min: f32, max: f32 },
    Fixed(f32),
    Percent(f32),
}

impl Slot {
    pub const fn fixed(mut self, width: f32, height: f32) -> Self {
        self.width = Sizing::fixed(width);
        self.height = Sizing::fixed(height);
        self
    }

    pub const fn grow(mut self) -> Self {
        self.width = Sizing::grow();
        self.height = Sizing::grow();
        self
    }
}

impl Sizing {
    pub const fn fit() -> Self {
        Self::Fit {
            min: 0.0,
            max: f32::INFINITY,
        }
    }

    pub const fn grow() -> Self {
        Self::Grow {
            min: 0.0,
            max: f32::INFINITY,
        }
    }

    pub const fn fixed(size: f32) -> Self {
        Self::Fixed(size)
    }

    pub const fn percent(fraction: f32) -> Self {
        Self::Percent(fraction)
    }

    pub const fn min(self, value: f32) -> Self {
        match self {
            Self::Fit { max, .. } => Self::Fit { min: value, max },
            Self::Grow { max, .. } => Self::Grow { min: value, max },
            Self::Fixed(_) | Self::Percent(_) => self,
        }
    }

    pub const fn max(self, value: f32) -> Self {
        match self {
            Self::Fit { min, .. } => Self::Fit { min, max: value },
            Self::Grow { min, .. } => Self::Grow { min, max: value },
            Self::Fixed(_) | Self::Percent(_) => self,
        }
    }

    pub fn resolve(self, intrinsic: f32, available: f32, cross: bool) -> f32 {
        match self {
            Self::Fit { .. } => self.clamp(intrinsic.min(available)),
            Self::Grow { .. } if cross => self.clamp(available),
            Self::Grow { .. } => self.clamp(intrinsic.min(available)),
            Self::Fixed(size) => size.max(0.0),
            Self::Percent(fraction) if available.is_finite() => {
                assert!((0.0..=1.0).contains(&fraction));
                available * fraction
            }
            Self::Percent(_) => 0.0,
        }
    }

    pub fn clamp(self, size: f32) -> f32 {
        match self {
            Self::Fit { min, max } | Self::Grow { min, max } => {
                size.clamp(min.max(0.0), max.max(min).max(0.0))
            }
            Self::Fixed(fixed) => fixed.max(0.0),
            Self::Percent(_) => size.max(0.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Absolute {
    pub target: PositionTarget,
    pub target_anchor: Anchor,
    pub child_anchor: Anchor,
    pub offset: Point,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PositionTarget {
    #[default]
    Parent,
    Node(NodeId),
    Screen,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Absolute {
    pub const fn at(x: f32, y: f32) -> Self {
        Self {
            target: PositionTarget::Parent,
            target_anchor: Anchor::TopLeft,
            child_anchor: Anchor::TopLeft,
            offset: Point::new(x, y),
        }
    }

    pub const fn screen(x: f32, y: f32) -> Self {
        Self {
            target: PositionTarget::Screen,
            ..Self::at(x, y)
        }
    }

    pub const fn attach(target: Anchor, child: Anchor) -> Self {
        Self::at(0.0, 0.0).anchors(target, child)
    }

    pub const fn relative_to(mut self, target: NodeId) -> Self {
        self.target = PositionTarget::Node(target);
        self
    }

    pub const fn anchors(mut self, target: Anchor, child: Anchor) -> Self {
        self.target_anchor = target;
        self.child_anchor = child;
        self
    }

    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = Point::new(x, y);
        self
    }
}

pub struct Leaves<'ui, R: Platform> {
    pub(super) ui: &'ui mut Ui<R>,
    pub(super) node: NodeId,
}

impl<R: Platform> Leaves<'_, R> {
    pub fn add<L: Leaf<R>>(&mut self, leaf: L) -> &mut Self {
        let frame = self.ui.frame_mut();
        let leaf = frame.store_leaf(leaf);
        frame.append_leaf(self.node, leaf);
        self
    }

    pub fn node(&self) -> NodeId {
        assert!(
            self.ui.frame().nodes[self.node.index()]
                .first_leaf
                .index()
                .is_some(),
            "leaf scope is empty"
        );
        self.node
    }
}

pub struct Container<'ui, R, L>
where
    R: Platform,
    L: Layout<R>,
{
    ui: &'ui mut Ui<R>,
    node: NodeId,
    marker: PhantomData<L>,
}

impl<'ui, R: Platform, L: Layout<R>> Container<'ui, R, L> {
    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn clip<C: Clip<R>>(self, clip: C) -> Self {
        let frame = self.ui.frame_mut();
        assert!(
            frame.nodes[self.node.index()].clip.index().is_none(),
            "layout already has a clip"
        );
        let clip = frame.store_clip(clip);
        frame.nodes[self.node.index()].clip = clip;
        self
    }

    pub fn absolute(self, absolute: Absolute) -> Self {
        self.ui.frame_mut().set_absolute(self.node, absolute);
        self
    }

    pub fn offset(self, offset: Point) -> Self {
        let frame = self.ui.frame_mut();
        let layout = frame.nodes[self.node.index()].layout.index().unwrap();
        frame.layouts[layout].offset = offset;
        self
    }

    pub fn id(self, id: WidgetId) -> Self {
        self.ui.frame_mut().set_id(self.node, id);
        self
    }

    pub fn hit(self, hit: Sides) -> Self {
        self.ui.frame_mut().set_hit(self.node, hit);
        self
    }

    pub fn transition(self, transition: Transition) -> Self {
        self.ui.frame_mut().set_transition(self.node, transition);
        self
    }

    pub fn new_layer(&mut self) -> LayerId {
        self.ui.frame_mut().add_layer()
    }

    pub fn item(&mut self, item: L::Item) -> ChildCx<'_, 'ui, R, L> {
        ChildCx {
            container: self,
            slot: Slot::new(),
            item,
            id: None,
        }
    }

    fn insert<W: Widget<R>>(
        &mut self,
        slot: Slot,
        item: L::Item,
        id: Option<WidgetId>,
        widget: W,
    ) -> W::Response {
        let start = self.ui.frame().nodes.len();
        let output = self.ui.add(widget);
        let frame = self.ui.frame_mut();
        let end = frame.nodes.len();
        assert!(end > start, "layout child did not add a node");

        let child = frame.node_id(start);
        assert_eq!(
            frame.nodes[child.index()].parent,
            self.node,
            "layout child was added outside its parent"
        );
        assert_eq!(
            frame.nodes[child.index()].subtree_end as usize + 1,
            end,
            "a layout item must contain exactly one root"
        );
        frame.set_slot(child, slot);
        let data = frame.data.store(item);
        frame.nodes[child.index()].item = data;
        if let Some(id) = id {
            frame.set_id(child, id);
        }
        output
    }
}

impl<'ui, R, L> Container<'ui, R, L>
where
    R: Platform,
    L: Layout<R, Item = ()>,
{
    pub fn add<W: Widget<R>>(&mut self, widget: W) -> W::Response {
        self.insert(Slot::new(), (), None, widget)
    }

    pub fn child(&mut self) -> ChildCx<'_, 'ui, R, L> {
        self.item(())
    }

    pub fn layout<N, O>(&mut self, layout: N, children: impl FnOnce(Container<'_, R, N>) -> O) -> O
    where
        N: Layout<R>,
    {
        self.child().layout(layout, children)
    }

    pub fn layout_with<B, N, O>(
        &mut self,
        base: B,
        layout: N,
        children: impl FnOnce(Container<'_, R, N>) -> O,
    ) -> O
    where
        B: Widget<R, Response = NodeId>,
        N: Layout<R>,
    {
        self.child().layout_with(base, layout, children)
    }
}

pub struct ChildCx<'child, 'ui, R, L>
where
    R: Platform,
    L: Layout<R>,
{
    container: &'child mut Container<'ui, R, L>,
    slot: Slot,
    item: L::Item,
    id: Option<WidgetId>,
}
impl<R: Platform, L: Layout<R>> ChildCx<'_, '_, R, L> {
    pub fn slot(mut self, slot: Slot) -> Self {
        self.slot = slot;
        self
    }

    pub fn id(mut self, id: WidgetId) -> Self {
        self.id = Some(id);
        self
    }

    #[allow(clippy::should_implement_trait)]
    pub fn add<W: Widget<R>>(self, widget: W) -> W::Response {
        self.container.insert(self.slot, self.item, self.id, widget)
    }

    pub fn layout<N, O>(self, layout: N, children: impl FnOnce(Container<'_, R, N>) -> O) -> O
    where
        N: Layout<R>,
    {
        self.add(|ui: &mut Ui<R>| children(ui.layout(layout)))
    }

    pub fn layout_with<B, N, O>(
        self,
        base: B,
        layout: N,
        children: impl FnOnce(Container<'_, R, N>) -> O,
    ) -> O
    where
        B: Widget<R, Response = NodeId>,
        N: Layout<R>,
    {
        self.add(|ui: &mut Ui<R>| children(ui.layout_with(base, layout)))
    }
}

impl<R: Platform, L: Layout<R>> Drop for Container<'_, R, L> {
    fn drop(&mut self) {
        let frame = self.ui.frame_mut();
        frame.nodes[self.node.index()].subtree_end =
            u32::try_from(frame.nodes.len() - 1).expect("too many frame nodes");
        let parent = frame.nodes[self.node.index()].parent;
        frame.current_parent = (parent != self.node).then_some(parent);
    }
}

pub(super) fn new<R: Platform, L: Layout<R>>(ui: &mut Ui<R>, node: NodeId) -> Container<'_, R, L> {
    ui.frame_mut().current_parent = Some(node);
    Container {
        ui,
        node,
        marker: PhantomData,
    }
}

pub fn layer_id(index: usize) -> LayerId {
    let value = u16::try_from(index + 1).expect("too many layers in one frame");
    LayerId(
        NonZeroU16::new(value).unwrap(),
        #[cfg(debug_assertions)]
        super::generation::get(),
    )
}

pub fn layer_index(id: LayerId) -> usize {
    #[cfg(debug_assertions)]
    super::generation::assert(id.1);
    id.0.get() as usize - 1
}

pub fn layer_order(id: LayerId) -> u16 {
    #[cfg(debug_assertions)]
    super::generation::assert(id.1);
    id.0.get()
}
