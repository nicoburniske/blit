use std::{marker::PhantomData, num::NonZeroU16};

use super::{Frame, NodeId, Ui, ui};
use crate::{
    animation::Transition,
    clip::Clip,
    geometry::{Point, Sides},
    interact::WidgetId,
    layout::Layout,
    renderer::Renderer,
};

/// frame-local paint layer
///
/// do not store this across renders
#[cfg_attr(not(debug_assertions), repr(transparent))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerId(NonZeroU16, #[cfg(debug_assertions)] u16);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    pub width: Sizing,
    pub height: Sizing,
    pub layer: Option<LayerId>,
    pub z_index: i16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    Fit { min: f32, max: f32 },
    Grow { min: f32, max: f32 },
    Fixed(f32),
    Percent(f32),
}

impl Default for Slot {
    fn default() -> Self {
        Self::new()
    }
}

impl Slot {
    pub const fn new() -> Self {
        Self {
            width: Sizing::fit(),
            height: Sizing::fit(),
            layer: None,
            z_index: 0,
        }
    }

    pub const fn width(mut self, width: Sizing) -> Self {
        self.width = width;
        self
    }

    pub const fn height(mut self, height: Sizing) -> Self {
        self.height = height;
        self
    }

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

    pub const fn layer(mut self, layer: LayerId) -> Self {
        self.layer = Some(layer);
        self
    }

    pub const fn z_index(mut self, z_index: i16) -> Self {
        self.z_index = z_index;
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

pub struct Container<'a, R, L>
where
    R: Renderer,
    L: Layout<R>,
{
    frame: &'a mut Frame<R>,
    node: NodeId,
    marker: PhantomData<L>,
}

impl<R: Renderer, L: Layout<R>> Container<'_, R, L> {
    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn clip<C: Clip<R>>(self, clip: C) -> Self {
        assert!(
            self.frame.nodes[self.node.index()].clip.is_none(),
            "layout already has a clip"
        );
        let clip = self.frame.store_clip(clip);
        self.frame.nodes[self.node.index()].clip = Some(clip);
        self
    }

    pub fn absolute(self, absolute: Absolute) -> Self {
        self.frame.set_absolute(self.node, absolute);
        self
    }

    pub fn offset(self, offset: Point) -> Self {
        self.frame.nodes[self.node.index()].content_offset = offset;
        self
    }

    pub fn id(self, id: WidgetId) -> Self {
        self.frame.nodes[self.node.index()].id = Some(id);
        self
    }

    pub fn hit(self, hit: Sides) -> Self {
        self.frame.nodes[self.node.index()].hit = hit;
        self
    }

    pub fn transition(self, transition: Transition) -> Self {
        self.frame.nodes[self.node.index()].transition = Some(transition);
        self
    }

    pub fn new_layer(&mut self) -> LayerId {
        self.frame.add_layer(self.node)
    }

    pub fn add<O>(&mut self, slot: Slot, item: L::Item, child: impl FnOnce(Ui<'_, R>) -> O) -> O {
        let start = self.frame.nodes.len();
        let output = child(ui::new(self.frame, Some(self.node)));
        let end = self.frame.nodes.len();
        assert!(end > start, "layout child did not add a node");

        let child = self.frame.node_id(start);
        assert_eq!(
            self.frame.nodes[child.index()].parent,
            Some(self.node),
            "layout child was added outside its parent"
        );
        assert_eq!(
            self.frame.nodes[child.index()].subtree_end as usize + 1,
            end,
            "a layout item must contain exactly one root"
        );
        self.frame.set_slot(child, slot);
        let data = self.frame.data.store(item);
        self.frame.nodes[child.index()].item = Some(data);
        output
    }
}

impl<R: Renderer, L: Layout<R>> Drop for Container<'_, R, L> {
    fn drop(&mut self) {
        self.frame.nodes[self.node.index()].subtree_end =
            u32::try_from(self.frame.nodes.len() - 1).expect("too many frame nodes");
    }
}

pub fn new<R: Renderer, L: Layout<R>>(frame: &mut Frame<R>, node: NodeId) -> Container<'_, R, L> {
    Container {
        frame,
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
