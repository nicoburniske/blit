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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerId(NonZeroU16);

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

    pub fn z_index(self, z_index: i16) -> Self {
        self.frame.nodes[self.node.index()].z_index = z_index;
        self.frame.needs_paint_order |= z_index != 0;
        self
    }

    pub fn layer(self, layer: LayerId) -> Self {
        let layer = layer_index(layer);
        assert!(
            layer < self.frame.layers.len(),
            "layer does not belong to this frame"
        );
        assert!(
            self.frame.layers[layer].owner.index() < self.node.index(),
            "a layer can only contain nodes declared after its owner"
        );
        self.frame.nodes[self.node.index()].layer = Some(layer_id(layer));
        self.frame.needs_paint_order = true;
        self
    }

    pub fn absolute(self, absolute: Absolute) -> Self {
        self.frame.set_absolute(self.node, absolute);
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

    pub fn add<O>(&mut self, item: L::Item, child: impl FnOnce(Ui<'_, R>) -> O) -> O {
        let start = self.frame.nodes.len();
        let output = child(ui::new(self.frame, Some(self.node)));
        let end = self.frame.nodes.len();
        assert!(end > start, "layout child did not add a node");

        let child = NodeId(start as u32);
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
    LayerId(NonZeroU16::new(value).unwrap())
}

pub fn layer_index(id: LayerId) -> usize {
    id.0.get() as usize - 1
}

pub fn layer_order(id: LayerId) -> u16 {
    id.0.get()
}
