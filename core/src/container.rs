//! frame-local child-bearing container configuration

use std::num::NonZeroU16;

use crate::{
    Ui,
    animation::Transition,
    geometry::{LogicalPoint, Sides},
    interact::WidgetId,
    layout::{Flex, Layout, RawScope},
    style::{Clip, Style},
};

/// pending child-bearing layout declaration
#[must_use = "container must be opened"]
pub struct Container<'ui, 'style, L = Flex> {
    ui: &'ui mut Ui,
    layout: L,
    config: ContainerConfig<'style>,
}

crate::builder! {
    /// configuration for a child-bearing layout scope
    pub struct ContainerConfig<'a> {
        new(),
        @optional {
            id: WidgetId,
            absolute: Absolute,
            transition: Transition,
        },
        slot: Slot = Slot::new(),
        offset: LogicalPoint = LogicalPoint { x: 0.0, y: 0.0 },
        hit: Sides = Sides::all(0.0),
        style: Style<'a> = Style::new(),
        clip: Clip = Clip::None,
    }
}

crate::builder! {
    /// sizing and paint placement of a widget in its parent
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

/// frame-local paint layer declared with [`Ui::layer`]
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerId(pub(crate) NonZeroU16);

/// sizing behavior on one axis
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sizing {
    Fit { min: f32, max: f32 },
    Grow { min: f32, max: f32 },
    Fixed(f32),
    Percent(f32),
}

impl<'ui, L> Container<'ui, 'static, L> {
    pub fn new(ui: &'ui mut Ui, layout: L) -> Self {
        Self {
            ui,
            config: ContainerConfig::default(),
            layout,
        }
    }
}

impl<'ui, 'style, L: Layout> Container<'ui, 'style, L> {
    pub fn slot(mut self, slot: Slot) -> Self {
        self.config.slot = slot;
        self
    }

    pub fn width(mut self, width: Sizing) -> Self {
        self.config.slot.width = width;
        self
    }

    pub fn height(mut self, height: Sizing) -> Self {
        self.config.slot.height = height;
        self
    }

    pub fn fixed(mut self, width: f32, height: f32) -> Self {
        self.config.slot.width = Sizing::fixed(width);
        self.config.slot.height = Sizing::fixed(height);
        self
    }

    pub fn grow(mut self) -> Self {
        self.config.slot.width = Sizing::grow();
        self.config.slot.height = Sizing::grow();
        self
    }

    /// sets this child's order among its paint siblings
    pub fn z_index(mut self, z_index: i16) -> Self {
        self.config.slot.z_index = z_index;
        self
    }

    /// paints this container and its descendants in a declared layer
    pub fn layer(mut self, layer: LayerId) -> Self {
        self.config.slot.layer = Some(layer);
        self
    }

    pub fn offset(mut self, offset: LogicalPoint) -> Self {
        self.config.offset = offset;
        self
    }

    pub fn id(mut self, id: WidgetId) -> Self {
        self.config.id = Some(id);
        self
    }

    /// extends this container's hit area without affecting layout or paint
    ///
    /// requires an [`id`](Self::id)
    pub fn hit(mut self, sides: Sides) -> Self {
        self.config.hit = sides;
        self
    }

    /// requires an [`id`](Self::id) to animate
    pub fn transition(mut self, transition: Transition) -> Self {
        self.config.transition = Some(transition);
        self
    }

    pub fn style(mut self, style: Style<'style>) -> Self {
        self.config.style = style;
        self
    }

    pub fn clip(mut self, clip: Clip) -> Self {
        self.config.clip = clip;
        self
    }

    pub fn absolute(mut self, absolute: Absolute) -> Self {
        self.config.absolute = Some(absolute);
        self
    }

    pub fn open(self) -> L::Scope<'ui> {
        let node = self.ui.open_layout(self.layout, self.config);
        L::Scope::from(RawScope {
            ui: self.ui,
            node,
            layout: self.layout,
        })
    }
}

impl Default for ContainerConfig<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for Slot {
    fn default() -> Self {
        Self::new()
    }
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

    /// sizes to a fraction of the parent's available size after padding and gaps
    ///
    /// percentage sizing does not contribute to a fitted parent's intrinsic size
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

    pub fn minimum(self, resolved: f32) -> f32 {
        match self {
            Self::Fit { min, .. } | Self::Grow { min, .. } => min.max(0.0),
            Self::Fixed(size) => size.max(0.0),
            Self::Percent(_) => resolved,
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
}

/// coordinate space used by absolute placement
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PositionTarget {
    #[default]
    Parent,
    Widget(WidgetId),
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
}
