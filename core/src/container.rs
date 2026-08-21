//! frame-local containers and flex layout

use std::ops::{Deref, DerefMut};

use crate::{
    Ui,
    geometry::{LogicalInsets, LogicalPoint},
    interact::WidgetId,
    node::NodeId,
    style::{Border, BorderRadius, Clip, LinearGradient, Shadow, Style},
};

/// pending child-bearing layout declaration
#[must_use = "container must be opened"]
pub struct Container<'ui, 'style> {
    ui: &'ui mut Ui,
    config: ContainerConfig<'style>,
    axis: Axis,
    absolute: Option<Absolute>,
}

/// opened child-bearing layout scope
pub struct Scope<'ui> {
    ui: &'ui mut Ui,
    node: NodeId,
}

/// configuration for a child-bearing layout scope
pub struct ContainerConfig<'a> {
    pub id: Option<WidgetId>,
    pub item: Item,
    pub padding: LogicalInsets,
    pub gap: f32,
    pub align: Align,
    pub justify: Justify,
    pub allow_overflow: bool,
    pub child_offset: LogicalPoint,
    pub style: Style<'a>,
    pub clip: Clip,
}

/// sizing applied to one item by its parent
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub width: Sizing,
    pub height: Sizing,
}

/// sizing behavior on one axis
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    Fit { min: f32, max: f32 },
    Grow { min: f32, max: f32 },
    Fixed(f32),
}

/// child flow direction
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    #[default]
    Vertical,
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

impl<'ui> Container<'ui, 'static> {
    pub fn new(ui: &'ui mut Ui) -> Self {
        Self {
            ui,
            config: ContainerConfig::default(),
            axis: Axis::Vertical,
            absolute: None,
        }
    }
}

impl<'ui, 'style> Container<'ui, 'style> {
    pub fn row(mut self) -> Self {
        self.axis = Axis::Horizontal;
        self
    }

    pub fn col(mut self) -> Self {
        self.axis = Axis::Vertical;
        self
    }

    pub fn flow(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }

    pub fn width(mut self, width: Sizing) -> Self {
        self.config.item.width = width;
        self
    }

    pub fn height(mut self, height: Sizing) -> Self {
        self.config.item.height = height;
        self
    }

    pub fn fixed(mut self, width: f32, height: f32) -> Self {
        self.config.item.width = Sizing::fixed(width);
        self.config.item.height = Sizing::fixed(height);
        self
    }

    pub fn grow(mut self) -> Self {
        self.config.item.width = Sizing::grow();
        self.config.item.height = Sizing::grow();
        self
    }

    pub fn padding(mut self, padding: LogicalInsets) -> Self {
        self.config.padding = padding;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.config.gap = gap;
        self
    }

    pub fn align(mut self, align: Align) -> Self {
        self.config.align = align;
        self
    }

    pub fn justify(mut self, justify: Justify) -> Self {
        self.config.justify = justify;
        self
    }

    pub fn overflow(mut self, overflow: bool) -> Self {
        self.config.allow_overflow = overflow;
        self
    }

    pub fn id(mut self, id: WidgetId) -> Self {
        self.config.id = Some(id);
        self
    }

    pub fn style(mut self, style: Style<'style>) -> Self {
        self.config.style = style;
        self
    }

    pub fn background(mut self, color: crate::color::Color) -> Self {
        self.config.style.background = color;
        self
    }

    pub fn border(mut self, width: f32, color: crate::color::Color) -> Self {
        self.config.style.border = Border::Solid { width, color };
        self
    }

    pub fn gradient_border(mut self, width: f32, gradient: LinearGradient<'style>) -> Self {
        self.config.style.border = Border::Gradient { width, gradient };
        self
    }

    pub fn radius(mut self, radius: BorderRadius) -> Self {
        self.config.style.radius = radius;
        self
    }

    pub fn uniform_radius(mut self, radius: f32) -> Self {
        self.config.style.radius = BorderRadius {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        };
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.config.style.opacity = opacity;
        self
    }

    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.config.style.shadow = Some(shadow);
        self
    }

    pub fn inset_shadow(mut self, shadow: Shadow) -> Self {
        self.config.style.inset_shadow = Some(shadow);
        self
    }

    pub fn clip(mut self, clip: Clip) -> Self {
        self.config.clip = clip;
        self
    }

    pub fn offset(mut self, offset: LogicalPoint) -> Self {
        self.config.child_offset = offset;
        self
    }

    pub fn absolute(mut self, absolute: Absolute) -> Self {
        self.absolute = Some(absolute);
        self
    }

    pub fn open(self) -> Scope<'ui> {
        let node = match self.absolute {
            Some(absolute) => self
                .ui
                .open_absolute_container(self.axis, self.config, absolute),
            None => self.ui.open_container(self.axis, self.config),
        };
        Scope { ui: self.ui, node }
    }
}

impl Default for ContainerConfig<'_> {
    fn default() -> Self {
        Self {
            id: None,
            item: Item::new(),
            padding: LogicalInsets {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            },
            gap: 0.0,
            align: Align::Stretch,
            justify: Justify::Start,
            allow_overflow: false,
            child_offset: LogicalPoint { x: 0.0, y: 0.0 },
            style: Style::new(),
            clip: Clip::None,
        }
    }
}

impl Item {
    pub const fn new() -> Self {
        Self {
            width: Sizing::fit(),
            height: Sizing::fit(),
        }
    }
}

impl Default for Item {
    fn default() -> Self {
        Self::new()
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

    pub const fn min(self, value: f32) -> Self {
        match self {
            Self::Fit { max, .. } => Self::Fit { min: value, max },
            Self::Grow { max, .. } => Self::Grow { min: value, max },
            Self::Fixed(_) => self,
        }
    }

    pub const fn max(self, value: f32) -> Self {
        match self {
            Self::Fit { min, .. } => Self::Fit { min, max: value },
            Self::Grow { min, .. } => Self::Grow { min, max: value },
            Self::Fixed(_) => self,
        }
    }
}

/// absolute placement of a container outside its parent's flow
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Absolute {
    pub target: PositionTarget,
    pub target_anchor: Anchor,
    pub child_anchor: Anchor,
    pub offset: LogicalPoint,
    pub z_index: i16,
}

/// coordinate space used by absolute placement
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PositionTarget {
    #[default]
    Parent,
    Screen,
}

/// point on a rectangle used for absolute attachment
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
            offset: LogicalPoint { x, y },
            z_index: 0,
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

    pub const fn relative_to(mut self, target: PositionTarget) -> Self {
        self.target = target;
        self
    }

    pub const fn anchors(mut self, target: Anchor, child: Anchor) -> Self {
        self.target_anchor = target;
        self.child_anchor = child;
        self
    }

    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = LogicalPoint { x, y };
        self
    }

    /// orders this absolute subtree relative to other layers
    ///
    /// negative layers paint below normal flow, while zero and positive layers
    /// paint above it. equal values retain declaration order
    pub const fn z_index(mut self, z_index: i16) -> Self {
        self.z_index = z_index;
        self
    }
}

impl Deref for Scope<'_> {
    type Target = Ui;

    fn deref(&self) -> &Self::Target {
        self.ui
    }
}

impl DerefMut for Scope<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ui
    }
}

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.ui.close_container(self.node)
    }
}
