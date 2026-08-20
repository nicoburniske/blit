//! frame-local containers and flex layout

use std::ops::{Deref, DerefMut};

use crate::{
    Ui,
    geometry::{LogicalInsets, LogicalPoint},
    graph::NodeId,
    interact::{Interaction, Sense, WidgetId},
    paint::{Border, BorderRadius, LinearGradient},
    style::{Appearance, Clip, Shadow},
};

/// configuration for a child-bearing layout scope
pub struct Container<'a> {
    pub id: Option<WidgetId>,
    pub item: Item,
    pub padding: LogicalInsets,
    pub gap: f32,
    pub align: Align,
    pub justify: Justify,
    pub allow_overflow: bool,
    pub child_offset: LogicalPoint,
    pub appearance: Appearance<'a>,
    pub clip: Clip,
    pub interaction: Option<(WidgetId, Sense)>,
}

/// scoped child declaration
pub struct Scope<'a> {
    ui: &'a mut Ui,
    node: NodeId,
    interaction: Interaction,
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

impl<'a> Container<'a> {
    pub const fn new() -> Self {
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
            appearance: Appearance::new(),
            clip: Clip::None,
            interaction: None,
        }
    }

    pub const fn width(mut self, width: Sizing) -> Self {
        self.item.width = width;
        self
    }

    pub const fn height(mut self, height: Sizing) -> Self {
        self.item.height = height;
        self
    }

    pub const fn fixed(mut self, width: f32, height: f32) -> Self {
        self.item.width = Sizing::fixed(width);
        self.item.height = Sizing::fixed(height);
        self
    }

    pub const fn grow(mut self) -> Self {
        self.item.width = Sizing::grow();
        self.item.height = Sizing::grow();
        self
    }

    pub const fn padding(mut self, padding: LogicalInsets) -> Self {
        self.padding = padding;
        self
    }

    pub const fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub const fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub const fn justify(mut self, justify: Justify) -> Self {
        self.justify = justify;
        self
    }

    pub const fn overflow(mut self, overflow: bool) -> Self {
        self.allow_overflow = overflow;
        self
    }

    pub const fn id(mut self, id: WidgetId) -> Self {
        self.id = Some(id);
        self
    }

    pub const fn appearance(mut self, appearance: Appearance<'a>) -> Self {
        self.appearance = appearance;
        self
    }

    pub const fn background(mut self, color: crate::color::Color) -> Self {
        self.appearance.background = color;
        self
    }

    pub const fn border(mut self, width: f32, color: crate::color::Color) -> Self {
        self.appearance.border = Border::Solid { width, color };
        self
    }

    pub const fn gradient_border(mut self, width: f32, gradient: LinearGradient<'a>) -> Self {
        self.appearance.border = Border::Gradient { width, gradient };
        self
    }

    pub const fn radius(mut self, radius: BorderRadius) -> Self {
        self.appearance.radius = radius;
        self
    }

    pub const fn uniform_radius(mut self, radius: f32) -> Self {
        self.appearance.radius = BorderRadius {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        };
        self
    }

    pub const fn opacity(mut self, opacity: f32) -> Self {
        self.appearance.opacity = opacity;
        self
    }

    pub const fn shadow(mut self, shadow: Shadow) -> Self {
        self.appearance.shadow = Some(shadow);
        self
    }

    pub const fn clip(mut self, clip: Clip) -> Self {
        self.clip = clip;
        self
    }

    pub const fn offset(mut self, offset: LogicalPoint) -> Self {
        self.child_offset = offset;
        self
    }

    pub const fn interact(mut self, id: WidgetId, sense: Sense) -> Self {
        self.interaction = Some((id, sense));
        self
    }
}

impl Default for Container<'_> {
    fn default() -> Self {
        Self::new()
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

impl Scope<'_> {
    pub fn interaction(&self) -> Interaction {
        self.interaction
    }

    pub fn set_appearance(&mut self, appearance: Appearance<'_>) {
        self.ui.set_node_appearance(self.node, appearance)
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

pub(crate) fn open<'a>(ui: &'a mut Ui, axis: Axis, container: Container<'_>) -> Scope<'a> {
    let (node, interaction) = ui.open_container(axis, container);
    Scope {
        ui,
        node,
        interaction,
    }
}
