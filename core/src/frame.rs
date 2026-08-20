//! frame-local containers, content, and flex layout

use std::ops::{Deref, DerefMut};

use crate::{
    Ui,
    color::Color,
    geometry::{LogicalInsets, LogicalPoint, LogicalSize},
    graph::NodeId,
    interact::{Interaction, Sense, WidgetId},
    paint::{
        Border, BorderRadius, ImageFit, ImageSampling, ImageTiling, LinearGradient, NineSlice,
        TextOptions, TextStyle,
    },
    resource::{ImageId, TextSource},
    widget::Widget,
};

/// sizing applied to one item by its parent
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub width: Sizing,
    pub height: Sizing,
}

/// configuration for a child-bearing layout scope
pub struct Container<'a> {
    pub(crate) id: Option<WidgetId>,
    pub(crate) item: Item,
    pub(crate) flow: Flow,
    pub(crate) appearance: Appearance<'a>,
    pub(crate) clip: Clip,
    pub(crate) interaction: Option<(WidgetId, Sense)>,
}

#[derive(Clone, Copy)]
pub(crate) struct Flow {
    pub axis: Axis,
    pub padding: LogicalInsets,
    pub gap: f32,
    pub align: Align,
    pub justify: Justify,
    pub overflow: bool,
    pub offset: LogicalPoint,
}

/// scoped child declaration
pub struct Scope<'a> {
    ui: &'a mut Ui,
    node: NodeId,
    interaction: Interaction,
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

/// paint emitted for a node's resolved bounds
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appearance<'a> {
    pub background: Color,
    pub border: Border<'a>,
    pub radius: BorderRadius,
    pub opacity: f32,
    pub replace: bool,
    pub shadow: Option<Shadow>,
}

/// box shadow relative to a node's resolved bounds
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub color: Color,
    pub radius: BorderRadius,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
}

/// clipping applied to a node's content and descendants
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Clip {
    #[default]
    None,
    Bounds,
    Rounded(BorderRadius),
}

/// intrinsic leaf content
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Content {
    Rectangle(Appearance<'static>),
    Text(TextContent),
    Image(ImageContent),
}

/// text resolved after its node width is known
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextContent {
    pub text: TextSource,
    pub color: Color,
    pub style: TextStyle,
    pub options: TextOptions,
    pub offset_x: f32,
    pub selection: Option<TextSelection>,
    pub caret: Option<TextCaret>,
}

/// selection painted behind text
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextSelection {
    pub start: usize,
    pub end: usize,
    pub color: Color,
}

/// caret painted over text
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextCaret {
    pub offset: usize,
    pub width: f32,
    pub color: Color,
}

/// image content and its intrinsic size
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageContent {
    pub image: ImageId,
    pub intrinsic: LogicalSize,
    pub fit: ImageFit,
    pub sampling: ImageSampling,
    pub opacity: f32,
    pub colorize: Option<Color>,
    pub nine_slice: Option<NineSlice>,
    pub horizontal_tiling: ImageTiling,
    pub vertical_tiling: ImageTiling,
}

impl<'a> Container<'a> {
    pub const fn new() -> Self {
        Self {
            id: None,
            item: Item::new(),
            flow: Flow::new(Axis::Vertical),
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
        self.flow.padding = padding;
        self
    }

    pub const fn gap(mut self, gap: f32) -> Self {
        self.flow.gap = gap;
        self
    }

    pub const fn align(mut self, align: Align) -> Self {
        self.flow.align = align;
        self
    }

    pub const fn justify(mut self, justify: Justify) -> Self {
        self.flow.justify = justify;
        self
    }

    pub const fn overflow(mut self, overflow: bool) -> Self {
        self.flow.overflow = overflow;
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

    pub const fn background(mut self, color: Color) -> Self {
        self.appearance.background = color;
        self
    }

    pub const fn border(mut self, width: f32, color: Color) -> Self {
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

    pub const fn replace(mut self, replace: bool) -> Self {
        self.appearance.replace = replace;
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
        self.flow.offset = offset;
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

impl Flow {
    const fn new(axis: Axis) -> Self {
        Self {
            axis,
            padding: LogicalInsets {
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            },
            gap: 0.0,
            align: Align::Stretch,
            justify: Justify::Start,
            overflow: false,
            offset: LogicalPoint { x: 0.0, y: 0.0 },
        }
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

impl<'a> Appearance<'a> {
    pub const fn new() -> Self {
        Self {
            background: Color::TRANSPARENT,
            border: Border::None,
            radius: BorderRadius {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
            opacity: 1.0,
            replace: false,
            shadow: None,
        }
    }

    pub const fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    pub const fn border(mut self, width: f32, color: Color) -> Self {
        self.border = Border::Solid { width, color };
        self
    }

    pub const fn gradient_border(mut self, width: f32, gradient: LinearGradient<'a>) -> Self {
        self.border = Border::Gradient { width, gradient };
        self
    }

    pub const fn radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }

    pub const fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    pub const fn replace(mut self, replace: bool) -> Self {
        self.replace = replace;
        self
    }

    pub const fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }
}

impl Default for Appearance<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Shadow {
    pub const fn new(color: Color) -> Self {
        Self {
            color,
            radius: BorderRadius {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            },
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
        }
    }

    pub const fn radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }

    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }

    pub const fn blur(mut self, blur: f32) -> Self {
        self.blur = blur;
        self
    }

    pub const fn spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }
}

impl Scope<'_> {
    pub fn add<W: Widget>(&mut self, widget: W) -> W::Output {
        widget.build(self.ui)
    }

    pub fn interaction(&self) -> Interaction {
        self.interaction
    }

    pub fn set_appearance(&mut self, appearance: Appearance<'_>) {
        self.ui.frame_mut().set_appearance(self.node, appearance)
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
        self.ui.close_node(self.node)
    }
}

pub(crate) fn open_container<'a>(
    ui: &'a mut Ui,
    axis: Axis,
    container: Container<'_>,
) -> Scope<'a> {
    let interaction = container
        .interaction
        .map_or_default(|(id, sense)| ui.widget_interaction(id, sense));
    let node = ui.frame_mut().add_container(axis, container);
    Scope {
        ui,
        node,
        interaction,
    }
}
