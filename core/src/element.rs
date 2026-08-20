//! frame-local elements and flex layout

use std::ops::{Deref, DerefMut};

use crate::{
    Ui,
    color::Color,
    command_list::{ClipId, CommandList},
    geometry::{LogicalInsets, LogicalPoint, LogicalRect, LogicalSize},
    interact::{Interaction, InteractionState, Sense, WidgetId},
    paint::{
        Border, BorderRadius, BoxShadow, ImageFit, ImageRequest, ImageSampling, ImageTiling,
        LinearGradient, NineSlice, Rectangle, TextLayoutRequest, TextOptions, TextRequest,
        TextStyle, TextWrap,
    },
    platform::Platform,
    resource::{ImageId, TextSource},
    widget::Widget,
};

/// one frame-local UI element
pub struct Element<'a> {
    id: Option<WidgetId>,
    layout: Layout,
    appearance: Appearance<'a>,
    content: Content,
    clip: Clip,
    offset: LogicalPoint,
    interaction: Option<(WidgetId, Sense)>,
}

/// scoped child declaration
pub struct ElementScope<'a> {
    ui: &'a mut Ui,
    node: NodeId,
    interaction: Interaction,
}

/// geometry of an element and its children
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    axis: Axis,
    width: Sizing,
    height: Sizing,
    padding: LogicalInsets,
    gap: f32,
    align: Align,
    justify: Justify,
    overflow: bool,
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

/// paint emitted for an element's resolved bounds
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appearance<'a> {
    background: Color,
    border: Border<'a>,
    radius: BorderRadius,
    opacity: f32,
    replace: bool,
    shadow: Option<Shadow>,
}

/// box shadow relative to an element's resolved bounds
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    color: Color,
    radius: BorderRadius,
    offset_x: f32,
    offset_y: f32,
    blur: f32,
    spread: f32,
}

/// clipping applied to an element's content and descendants
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
    None,
    Text(TextContent),
    Image(ImageContent),
}

/// text resolved after its element width is known
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

/// geometry resolved for a stable element ID in the previous frame
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ElementGeometry {
    pub area: LogicalRect,
    pub content: LogicalSize,
}

impl<'a> Element<'a> {
    pub const fn new(layout: Layout) -> Self {
        Self {
            id: None,
            layout,
            appearance: Appearance::new(),
            content: Content::None,
            clip: Clip::None,
            offset: LogicalPoint { x: 0.0, y: 0.0 },
            interaction: None,
        }
    }

    pub const fn id(mut self, id: WidgetId) -> Self {
        self.id = Some(id);
        self
    }

    pub const fn content(mut self, content: Content) -> Self {
        self.content = content;
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

    /// offsets descendants without changing this element's layout
    pub const fn offset(mut self, offset: LogicalPoint) -> Self {
        self.offset = offset;
        self
    }

    pub const fn interact(mut self, id: WidgetId, sense: Sense) -> Self {
        self.id = Some(id);
        self.interaction = Some((id, sense));
        self
    }
}

impl Layout {
    pub const fn horizontal() -> Self {
        Self {
            axis: Axis::Horizontal,
            ..Self::vertical()
        }
    }

    pub const fn vertical() -> Self {
        Self {
            axis: Axis::Vertical,
            width: Sizing::fit(),
            height: Sizing::fit(),
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

    /// lets children retain their natural size beyond the flow axis
    pub const fn overflow(mut self, overflow: bool) -> Self {
        self.overflow = overflow;
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

impl ElementScope<'_> {
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

impl Deref for ElementScope<'_> {
    type Target = Ui;

    fn deref(&self) -> &Self::Target {
        self.ui
    }
}

impl DerefMut for ElementScope<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.ui
    }
}

impl Drop for ElementScope<'_> {
    fn drop(&mut self) {
        self.ui.close_element(self.node)
    }
}

// internals

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NodeId(usize);

#[derive(Default)]
pub(crate) struct FrameGraph {
    nodes: Vec<Node>,
    open: Vec<NodeId>,
    scratch: Vec<NodeId>,
    gradient_stops: Vec<crate::paint::GradientStop>,
}

struct Node {
    id: Option<WidgetId>,
    first_child: Option<NodeId>,
    last_child: Option<NodeId>,
    next_sibling: Option<NodeId>,
    layout: Layout,
    appearance: StoredAppearance,
    content: Content,
    clip_children: Clip,
    offset: LogicalPoint,
    interaction: Option<(WidgetId, Sense)>,
    intrinsic: LogicalSize,
    area: LogicalRect,
    clip: ClipId,
    content_clip: ClipId,
    clip_bounds: LogicalRect,
    content_clip_bounds: LogicalRect,
}

struct StoredAppearance {
    background: Color,
    border: StoredBorder,
    radius: BorderRadius,
    opacity: f32,
    replace: bool,
    shadow: Option<Shadow>,
}

enum StoredBorder {
    None,
    Solid {
        width: f32,
        color: Color,
    },
    Gradient {
        width: f32,
        angle_degrees: f32,
        start: usize,
        len: usize,
    },
}

impl FrameGraph {
    pub fn begin(&mut self, screen: LogicalRect) {
        self.nodes.clear();
        self.open.clear();
        self.scratch.clear();
        self.gradient_stops.clear();
        self.nodes.push(Node {
            first_child: None,
            id: None,
            last_child: None,
            next_sibling: None,
            layout: Layout::vertical()
                .width(Sizing::fixed(screen.width))
                .height(Sizing::fixed(screen.height)),
            appearance: StoredAppearance::default(),
            content: Content::None,
            clip_children: Clip::None,
            offset: LogicalPoint::default(),
            interaction: None,
            intrinsic: LogicalSize::default(),
            area: screen,
            clip: ClipId::default(),
            content_clip: ClipId::default(),
            clip_bounds: screen,
            content_clip_bounds: screen,
        });
        self.open.push(NodeId(0));
    }

    pub fn push(&mut self, element: Element<'_>) -> NodeId {
        let parent = *self
            .open
            .last()
            .expect("element declaration requires a root");
        let node = NodeId(self.nodes.len());
        let appearance = self.store_appearance(element.appearance);
        self.nodes.push(Node {
            id: element.id,
            first_child: None,
            last_child: None,
            next_sibling: None,
            layout: element.layout,
            appearance,
            content: element.content,
            clip_children: element.clip,
            offset: element.offset,
            interaction: element.interaction,
            intrinsic: LogicalSize::default(),
            area: LogicalRect::default(),
            clip: ClipId::default(),
            content_clip: ClipId::default(),
            clip_bounds: LogicalRect::default(),
            content_clip_bounds: LogicalRect::default(),
        });
        if let Some(last) = self.nodes[parent.0].last_child {
            self.nodes[last.0].next_sibling = Some(node);
        } else {
            self.nodes[parent.0].first_child = Some(node);
        }
        self.nodes[parent.0].last_child = Some(node);
        self.open.push(node);
        node
    }

    pub fn set_appearance(&mut self, node: NodeId, appearance: Appearance<'_>) {
        let appearance = self.store_appearance(appearance);
        self.nodes[node.0].appearance = appearance;
    }

    fn store_appearance(&mut self, appearance: Appearance<'_>) -> StoredAppearance {
        let border = match appearance.border {
            Border::None => StoredBorder::None,
            Border::Solid { width, color } => StoredBorder::Solid { width, color },
            Border::Gradient { width, gradient } => {
                let start = self.gradient_stops.len();
                self.gradient_stops.extend_from_slice(gradient.stops);
                StoredBorder::Gradient {
                    width,
                    angle_degrees: gradient.angle_degrees,
                    start,
                    len: gradient.stops.len(),
                }
            }
        };
        StoredAppearance {
            background: appearance.background,
            border,
            radius: appearance.radius,
            opacity: appearance.opacity,
            replace: appearance.replace,
            shadow: appearance.shadow,
        }
    }

    pub fn close(&mut self, node: NodeId) {
        assert_eq!(self.open.pop(), Some(node), "elements must close in order");
    }

    pub fn finish(
        &mut self,
        platform: &mut Platform,
        commands: &mut CommandList,
        interaction: &mut InteractionState,
        geometry: &mut GeometryState,
        scale_factor: f32,
    ) {
        assert_eq!(self.open, [NodeId(0)], "an element scope was not dropped");
        self.measure_intrinsic(platform);
        self.resolve_axis(Axis::Horizontal);
        self.measure_wrapped_text(platform);
        self.measure_container_heights();
        self.resolve_axis(Axis::Vertical);
        self.resolve_clips(commands);
        self.emit(platform, commands, scale_factor);
        self.register_hits(interaction, scale_factor);
        for node in &self.nodes {
            if let Some(id) = node.id {
                geometry.register(
                    id,
                    ElementGeometry {
                        area: node.area,
                        content: node.intrinsic,
                    },
                );
            }
        }
        self.open.clear();
    }

    fn measure_intrinsic(&mut self, platform: &mut Platform) {
        for node in &mut self.nodes {
            node.intrinsic = match node.content {
                Content::None => LogicalSize::default(),
                Content::Text(text) => platform.measure_text(&TextLayoutRequest {
                    text: text.text,
                    style: text.style,
                    wrap: TextWrap::None,
                    max_width: None,
                    max_lines: text.options.max_lines,
                }),
                Content::Image(image) => image.intrinsic,
            };
        }
        for index in (0..self.nodes.len()).rev() {
            self.measure_container(NodeId(index), true);
        }
    }

    fn measure_wrapped_text(&mut self, platform: &mut Platform) {
        for node in &mut self.nodes {
            let Content::Text(text) = node.content else {
                continue;
            };
            node.intrinsic = platform.measure_text(&TextLayoutRequest {
                text: text.text,
                style: text.style,
                wrap: text.options.wrap,
                max_width: (text.options.wrap != TextWrap::None).then_some(node.area.width),
                max_lines: text.options.max_lines,
            });
        }
    }

    fn measure_container_heights(&mut self) {
        for index in (0..self.nodes.len()).rev() {
            self.measure_container(NodeId(index), false);
        }
    }

    fn measure_container(&mut self, id: NodeId, width: bool) {
        let Some(mut child) = self.nodes[id.0].first_child else {
            return;
        };
        let layout = self.nodes[id.0].layout;
        let mut main: f32 = 0.0;
        let mut cross: f32 = 0.0;
        let mut count = 0usize;
        loop {
            let node = &self.nodes[child.0];
            let (main_axis, cross_axis) = match layout.axis {
                Axis::Horizontal => (Axis::Horizontal, Axis::Vertical),
                Axis::Vertical => (Axis::Vertical, Axis::Horizontal),
            };
            let child_main =
                node.sizing(main_axis)
                    .resolve(node.intrinsic(main_axis), f32::INFINITY, false);
            let child_cross =
                node.sizing(cross_axis)
                    .resolve(node.intrinsic(cross_axis), f32::INFINITY, true);
            main += child_main;
            cross = cross.max(child_cross);
            count += 1;
            let Some(next) = self.nodes[child.0].next_sibling else {
                break;
            };
            child = next;
        }
        main += layout.gap.max(0.0) * count.saturating_sub(1) as f32;
        let measured = match layout.axis {
            Axis::Horizontal => LogicalSize {
                width: main + layout.padding.left + layout.padding.right,
                height: cross + layout.padding.top + layout.padding.bottom,
            },
            Axis::Vertical => LogicalSize {
                width: cross + layout.padding.left + layout.padding.right,
                height: main + layout.padding.top + layout.padding.bottom,
            },
        };
        if width {
            self.nodes[id.0].intrinsic.width = measured.width;
        } else {
            self.nodes[id.0].intrinsic.height = measured.height;
        }
    }

    fn resolve_axis(&mut self, axis: Axis) {
        self.scratch.clear();
        self.scratch.push(NodeId(0));
        while let Some(parent) = self.scratch.pop() {
            self.resolve_children(parent, axis);
            let mut child = self.nodes[parent.0].first_child;
            while let Some(id) = child {
                self.scratch.push(id);
                child = self.nodes[id.0].next_sibling;
            }
        }
    }

    fn resolve_children(&mut self, parent: NodeId, axis: Axis) {
        let layout = self.nodes[parent.0].layout;
        let Some(mut child) = self.nodes[parent.0].first_child else {
            return;
        };
        let parent_area = self.nodes[parent.0].area;
        let (origin, available, flow, leading, trailing) = match axis {
            Axis::Horizontal => (
                parent_area.x + self.nodes[parent.0].offset.x,
                parent_area.width,
                layout.axis == Axis::Horizontal,
                layout.padding.left,
                layout.padding.right,
            ),
            Axis::Vertical => (
                parent_area.y + self.nodes[parent.0].offset.y,
                parent_area.height,
                layout.axis == Axis::Vertical,
                layout.padding.top,
                layout.padding.bottom,
            ),
        };
        let available = (available - leading - trailing).max(0.0);

        if !flow {
            loop {
                let sizing = self.nodes[child.0].sizing(axis);
                let intrinsic = self.nodes[child.0].intrinsic(axis);
                let mut size = sizing.resolve(intrinsic, available, true);
                if layout.align == Align::Stretch && matches!(sizing, Sizing::Fit { .. }) {
                    size = sizing.clamp(available);
                }
                let offset = match layout.align {
                    Align::Start | Align::Stretch => 0.0,
                    Align::Center => (available - size).max(0.0) / 2.0,
                    Align::End => (available - size).max(0.0),
                };
                self.nodes[child.0].set_axis(axis, origin + leading + offset, size);
                let Some(next) = self.nodes[child.0].next_sibling else {
                    break;
                };
                child = next;
            }
            return;
        }

        let mut count = 0usize;
        let mut grow = 0usize;
        let mut used = 0.0;
        loop {
            let node = &mut self.nodes[child.0];
            let sizing = node.sizing(axis);
            let size = sizing.resolve(
                node.intrinsic(axis),
                if layout.overflow {
                    f32::INFINITY
                } else {
                    available
                },
                false,
            );
            node.set_size(axis, size);
            used += size;
            count += 1;
            grow += usize::from(matches!(sizing, Sizing::Grow { .. }));
            let Some(next) = node.next_sibling else {
                break;
            };
            child = next;
        }
        let gaps = layout.gap.max(0.0) * count.saturating_sub(1) as f32;
        let free = available - used - gaps;
        if free < 0.0 && !layout.overflow {
            let mut capacity = 0.0;
            child = self.nodes[parent.0].first_child.unwrap();
            loop {
                let node = &self.nodes[child.0];
                capacity += node.size(axis) - node.sizing(axis).minimum();
                let Some(next) = node.next_sibling else {
                    break;
                };
                child = next;
            }
            if capacity > 0.0 {
                let deficit = (-free).min(capacity);
                child = self.nodes[parent.0].first_child.unwrap();
                loop {
                    let node = &mut self.nodes[child.0];
                    let available_shrink = node.size(axis) - node.sizing(axis).minimum();
                    node.set_size(
                        axis,
                        node.size(axis) - deficit * available_shrink / capacity,
                    );
                    let Some(next) = node.next_sibling else {
                        break;
                    };
                    child = next;
                }
            }
        }
        let free = free.max(0.0);
        if grow != 0 {
            let share = free / grow as f32;
            child = self.nodes[parent.0].first_child.unwrap();
            loop {
                let node = &mut self.nodes[child.0];
                if let Sizing::Grow { .. } = node.sizing(axis) {
                    node.set_size(axis, node.sizing(axis).clamp(node.size(axis) + share));
                }
                let Some(next) = node.next_sibling else {
                    break;
                };
                child = next;
            }
        }

        let used = self.children_size(parent, axis) + gaps;
        let remaining = (available - used).max(0.0);
        let (offset, extra_gap) = match layout.justify {
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
        };
        let mut cursor = origin + leading + offset;
        child = self.nodes[parent.0].first_child.unwrap();
        loop {
            let size = self.nodes[child.0].size(axis);
            self.nodes[child.0].set_axis(axis, cursor, size);
            cursor += size + layout.gap.max(0.0) + extra_gap;
            let Some(next) = self.nodes[child.0].next_sibling else {
                break;
            };
            child = next;
        }
    }

    fn children_size(&self, parent: NodeId, axis: Axis) -> f32 {
        let mut total = 0.0;
        let mut child = self.nodes[parent.0].first_child;
        while let Some(id) = child {
            total += self.nodes[id.0].size(axis);
            child = self.nodes[id.0].next_sibling;
        }
        total
    }

    fn resolve_clips(&mut self, commands: &mut CommandList) {
        self.nodes[0].clip = ClipId::default();
        for index in 0..self.nodes.len() {
            let child_clip = match self.nodes[index].clip_children {
                Clip::None => self.nodes[index].clip,
                Clip::Bounds => commands.push_clip(
                    self.nodes[index].clip,
                    self.nodes[index].area,
                    BorderRadius::default(),
                ),
                Clip::Rounded(radius) => {
                    commands.push_clip(self.nodes[index].clip, self.nodes[index].area, radius)
                }
            };
            self.nodes[index].content_clip = child_clip;
            self.nodes[index].content_clip_bounds = match self.nodes[index].clip_children {
                Clip::None => self.nodes[index].clip_bounds,
                Clip::Bounds | Clip::Rounded(_) => self.nodes[index]
                    .clip_bounds
                    .intersection(self.nodes[index].area)
                    .unwrap_or_default(),
            };
            let mut child = self.nodes[index].first_child;
            while let Some(child_id) = child {
                self.nodes[child_id.0].clip = child_clip;
                self.nodes[child_id.0].clip_bounds = self.nodes[index].content_clip_bounds;
                child = self.nodes[child_id.0].next_sibling;
            }
        }
    }

    fn emit(&self, platform: &mut Platform, commands: &mut CommandList, scale_factor: f32) {
        for node in self.nodes.iter().skip(1) {
            if let Some(shadow) = node.appearance.shadow {
                let shadow = BoxShadow::new(node.area, shadow.color)
                    .radius(shadow.radius)
                    .offset(shadow.offset_x, shadow.offset_y)
                    .blur(shadow.blur)
                    .spread(shadow.spread);
                if let Some(bounds) = node.visible_bounds(shadow.bounds(), false, scale_factor) {
                    commands.push_box_shadow(shadow, bounds, node.clip);
                }
            }
            if node.appearance.paints() {
                let border = match node.appearance.border {
                    StoredBorder::None => Border::None,
                    StoredBorder::Solid { width, color } => Border::Solid { width, color },
                    StoredBorder::Gradient {
                        width,
                        angle_degrees,
                        start,
                        len,
                    } => Border::Gradient {
                        width,
                        gradient: LinearGradient::new(&self.gradient_stops[start..start + len])
                            .angle(angle_degrees),
                    },
                };
                let rectangle = Rectangle {
                    area: node.area,
                    background: node.appearance.background,
                    border,
                    radius: node.appearance.radius,
                    opacity: node.appearance.opacity,
                    replace: node.appearance.replace,
                };
                if let Some(bounds) = node.visible_bounds(node.area, false, scale_factor) {
                    commands.push_rectangle(rectangle, bounds, node.clip);
                }
            }
            match node.content {
                Content::None => {}
                Content::Text(text) => {
                    let request = TextRequest {
                        text: text.text,
                        area: node.area,
                        offset_x: text.offset_x,
                        color: text.color,
                        style: text.style,
                        options: text.options,
                    };
                    if let Some(selection) = text.selection {
                        let start = platform.text_cursor_rect(&request, selection.start);
                        let end = platform.text_cursor_rect(&request, selection.end);
                        let left = start.x.max(node.area.x);
                        let right = end.x.min(node.area.x + node.area.width);
                        let top = start.y.max(node.area.y);
                        let bottom = (start.y + start.height).min(node.area.y + node.area.height);
                        if right > left && bottom > top {
                            let area = LogicalRect {
                                x: left,
                                y: top,
                                width: right - left,
                                height: bottom - top,
                            };
                            if let Some(bounds) = node.visible_bounds(area, true, scale_factor) {
                                commands.push_rectangle(
                                    Rectangle::new(area).background(selection.color),
                                    bounds,
                                    node.content_clip,
                                );
                            }
                        }
                    }
                    if let Some(bounds) = node.visible_bounds(node.area, true, scale_factor) {
                        commands.push_text(request, bounds, node.content_clip);
                    }
                    if let Some(caret) = text.caret {
                        let cursor = platform.text_cursor_rect(&request, caret.offset);
                        let x = cursor.x.clamp(
                            node.area.x,
                            (node.area.x + node.area.width - caret.width).max(node.area.x),
                        );
                        let top = cursor.y.max(node.area.y);
                        let bottom = (cursor.y + cursor.height).min(node.area.y + node.area.height);
                        if bottom > top {
                            let area = LogicalRect {
                                x,
                                y: top,
                                width: caret.width.min(node.area.width),
                                height: bottom - top,
                            };
                            if let Some(bounds) = node.visible_bounds(area, true, scale_factor) {
                                commands.push_rectangle(
                                    Rectangle::new(area).background(caret.color),
                                    bounds,
                                    node.content_clip,
                                );
                            }
                        }
                    }
                }
                Content::Image(image) => {
                    if let Some(bounds) = node.visible_bounds(node.area, true, scale_factor) {
                        commands.push_image(
                            ImageRequest {
                                image: image.image,
                                area: node.area,
                                fit: image.fit,
                                sampling: image.sampling,
                                opacity: image.opacity,
                                colorize: image.colorize,
                                nine_slice: image.nine_slice,
                                horizontal_tiling: image.horizontal_tiling,
                                vertical_tiling: image.vertical_tiling,
                            },
                            bounds,
                            node.content_clip,
                        );
                    }
                }
            }
        }
    }

    fn register_hits(&self, interaction: &mut InteractionState, scale_factor: f32) {
        for node in &self.nodes {
            let Some((id, sense)) = node.interaction else {
                continue;
            };
            let area = node
                .area
                .intersection(node.clip_bounds)
                .map(|area| area.to_physical(scale_factor));
            interaction.register(id, area, sense);
        }
    }
}

impl Node {
    fn visible_bounds(
        &self,
        area: LogicalRect,
        content: bool,
        scale_factor: f32,
    ) -> Option<crate::geometry::PhysicalRect> {
        area.intersection(if content {
            self.content_clip_bounds
        } else {
            self.clip_bounds
        })
        .map(|area| area.to_physical(scale_factor))
    }

    fn sizing(&self, axis: Axis) -> Sizing {
        match axis {
            Axis::Horizontal => self.layout.width,
            Axis::Vertical => self.layout.height,
        }
    }

    fn intrinsic(&self, axis: Axis) -> f32 {
        match axis {
            Axis::Horizontal => self.intrinsic.width,
            Axis::Vertical => self.intrinsic.height,
        }
    }

    fn size(&self, axis: Axis) -> f32 {
        match axis {
            Axis::Horizontal => self.area.width,
            Axis::Vertical => self.area.height,
        }
    }

    fn set_size(&mut self, axis: Axis, size: f32) {
        match axis {
            Axis::Horizontal => self.area.width = size,
            Axis::Vertical => self.area.height = size,
        }
    }

    fn set_axis(&mut self, axis: Axis, position: f32, size: f32) {
        match axis {
            Axis::Horizontal => {
                self.area.x = position;
                self.area.width = size;
            }
            Axis::Vertical => {
                self.area.y = position;
                self.area.height = size;
            }
        }
    }
}

impl Sizing {
    fn resolve(self, intrinsic: f32, available: f32, cross: bool) -> f32 {
        match self {
            Self::Fit { .. } => self.clamp(intrinsic.min(available)),
            Self::Grow { .. } if cross => self.clamp(available),
            Self::Grow { .. } => self.clamp(intrinsic.min(available)),
            Self::Fixed(size) => size.max(0.0).min(available),
        }
    }

    fn clamp(self, size: f32) -> f32 {
        match self {
            Self::Fit { min, max } | Self::Grow { min, max } => {
                size.clamp(min.max(0.0), max.max(min).max(0.0))
            }
            Self::Fixed(fixed) => fixed.max(0.0),
        }
    }

    fn minimum(self) -> f32 {
        match self {
            Self::Fit { min, .. } | Self::Grow { min, .. } => min.max(0.0),
            Self::Fixed(size) => size.max(0.0),
        }
    }
}

impl StoredAppearance {
    fn paints(&self) -> bool {
        self.background != Color::TRANSPARENT
            || !matches!(self.border, StoredBorder::None)
            || self.replace
    }
}

impl Default for StoredAppearance {
    fn default() -> Self {
        Self {
            background: Color::TRANSPARENT,
            border: StoredBorder::None,
            radius: BorderRadius::default(),
            opacity: 1.0,
            replace: false,
            shadow: None,
        }
    }
}

#[derive(Default)]
pub(crate) struct GeometryState {
    previous: Vec<(WidgetId, ElementGeometry)>,
    current: Vec<(WidgetId, ElementGeometry)>,
}

impl GeometryState {
    pub fn get(&self, id: WidgetId) -> Option<ElementGeometry> {
        self.previous
            .iter()
            .find_map(|(candidate, geometry)| (*candidate == id).then_some(*geometry))
    }

    pub fn register(&mut self, id: WidgetId, geometry: ElementGeometry) {
        self.current.push((id, geometry));
    }

    pub fn end_frame(&mut self) {
        std::mem::swap(&mut self.previous, &mut self.current);
        self.current.clear();
    }
}

pub(crate) fn open<'a>(ui: &'a mut Ui, element: Element<'_>) -> ElementScope<'a> {
    let interaction = element
        .interaction
        .map_or_default(|(id, sense)| ui.element_interaction(id, sense));
    let node = ui.frame_mut().push(element);
    ElementScope {
        ui,
        node,
        interaction,
    }
}
