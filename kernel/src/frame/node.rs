/// frame-local paint layer
///
/// do not store this across renders
#[cfg_attr(not(debug_assertions), repr(transparent))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerId(NonZeroU16, #[cfg(debug_assertions)] u16);

crate::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Place {
        new(),
        @optional {
            layer: LayerId,
        },
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
        z_index: i16 = 0,
    }
}

impl Place {
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    Fit { min: f32, max: f32 },
    Grow { min: f32, max: f32 },
    Fixed(f32),
    Percent(f32),
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

    #[inline]
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

    #[inline]
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

pub struct Node<'ui, R, L>
where
    R: Platform,
    L: Layout<R>,
{
    ui: &'ui mut Ui<R>,
    node: NodeId,
    marker: PhantomData<L>,
}

impl<'ui, R: Platform, L: Layout<R>> Node<'ui, R, L> {
    pub fn id(&self) -> NodeId {
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

    pub fn widget_id(self, id: WidgetId) -> Self {
        self.ui.frame_mut().set_id(self.node, id);
        self
    }

    pub fn hit(self, hit: Sides) -> Self {
        self.ui.frame_mut().set_hit(self.node, hit);
        self
    }

    /// inserts a widget into the current node
    pub fn insert<W: Widget<R>>(&mut self, widget: W) -> W::Response {
        widget.build(Cx {
            ui: self.ui,
            node: self.node,
        })
    }

    pub fn transition(self, transition: Transition) -> Self {
        self.ui.frame_mut().set_transition(self.node, transition);
        self
    }

    pub fn new_layer(&mut self) -> LayerId {
        self.ui.frame_mut().add_layer()
    }

    pub fn place(&mut self, place: Place) -> Child<'_, 'ui, R, L> {
        let node = self.ui.frame_mut().push_node(None);
        Child {
            parent: self,
            node,
            place,
            item: (),
            id: None,
        }
    }

    pub fn item(&mut self, item: L::Item) -> Child<'_, 'ui, R, L, L::Item> {
        self.place(Place::new()).item(item)
    }
}

impl<'ui, R: Platform, L: Layout<R, Item = ()>> Node<'ui, R, L> {
    /// adds a child widget with default placement
    pub fn add<W: Widget<R>>(&mut self, widget: W) -> W::Response {
        self.child().add(widget)
    }

    pub fn child(&mut self) -> Child<'_, 'ui, R, L> {
        self.place(Place::new())
    }

    /// adds a child layout node with default placement
    pub fn node<N: Layout<R>>(&mut self, layout: N) -> Node<'_, R, N> {
        self.child().node(layout)
    }
}

/// pending child insertion
#[must_use = "a child must be populated with add or node"]
pub struct Child<'entry, 'ui, R, L, I = ()>
where
    R: Platform,
    L: Layout<R>,
{
    parent: &'entry mut Node<'ui, R, L>,
    node: NodeId,
    place: Place,
    item: I,
    id: Option<WidgetId>,
}

impl<'entry, 'ui, R: Platform, L: Layout<R>, I> Child<'entry, 'ui, R, L, I> {
    pub fn id(&self) -> NodeId {
        self.node
    }

    pub fn item(self, item: L::Item) -> Child<'entry, 'ui, R, L, L::Item> {
        Child {
            parent: self.parent,
            node: self.node,
            place: self.place,
            item,
            id: self.id,
        }
    }

    pub fn place(mut self, place: Place) -> Self {
        self.place = place;
        self
    }

    pub fn widget_id(mut self, id: WidgetId) -> Self {
        self.id = Some(id);
        self
    }
}

impl<'entry, R: Platform, L: Layout<R>> Child<'entry, '_, R, L, L::Item> {
    /// adds a child widget
    pub fn add<W: Widget<R>>(self, widget: W) -> W::Response {
        let response = self.parent.ui.build_node(self.node, widget);
        let frame = self.parent.ui.frame_mut();
        frame.set_place(self.node, self.place);
        let item = frame.data.store(self.item);
        frame.nodes[self.node.index()].item = item;
        if let Some(id) = self.id {
            frame.set_id(self.node, id);
        }
        response
    }

    /// creates a child with a layout
    pub fn node<N: Layout<R>>(self, layout: N) -> Node<'entry, R, N> {
        let frame = self.parent.ui.frame_mut();
        let layout = frame.store_layout(layout);
        frame.nodes[self.node.index()].layout = layout;
        frame.set_place(self.node, self.place);
        let item = frame.data.store(self.item);
        frame.nodes[self.node.index()].item = item;
        if let Some(id) = self.id {
            frame.set_id(self.node, id);
        }
        new_node(self.parent.ui, self.node)
    }
}

impl<R: Platform, L: Layout<R>> Drop for Node<'_, R, L> {
    fn drop(&mut self) {
        let frame = self.ui.frame_mut();
        frame.nodes[self.node.index()].subtree_end =
            u32::try_from(frame.nodes.len() - 1).expect("too many frame nodes");
        let parent = frame.nodes[self.node.index()].parent;
        frame.current_parent = (parent != self.node).then_some(parent);
    }
}

fn new_node<R: Platform, L: Layout<R>>(ui: &mut Ui<R>, node: NodeId) -> Node<'_, R, L> {
    ui.frame_mut().current_parent = Some(node);
    Node {
        ui,
        node,
        marker: PhantomData,
    }
}

fn layer_id(index: usize) -> LayerId {
    let value = u16::try_from(index + 1).expect("too many layers in one frame");
    LayerId(
        NonZeroU16::new(value).unwrap(),
        #[cfg(debug_assertions)]
        generation::get(),
    )
}

fn layer_index(id: LayerId) -> usize {
    #[cfg(debug_assertions)]
    generation::assert(id.1);
    id.0.get() as usize - 1
}

fn layer_order(id: LayerId) -> u16 {
    #[cfg(debug_assertions)]
    generation::assert(id.1);
    id.0.get()
}
