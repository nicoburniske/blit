use crate::{
    FrameGraphMemory,
    color::Color,
    command_list::{ClipId, CommandList},
    container::{Align, Axis, Container, Item, Justify, Sizing},
    geometry::{LogicalInsets, LogicalPoint, LogicalRect, LogicalSize},
    interact::{InteractionState, Sense, WidgetId},
    paint::{
        Border, BorderRadius, BoxShadow, ImageFit, ImageRequest, ImageSampling, ImageTiling,
        LinearGradient, NineSlice, Rectangle, TextLayoutRequest, TextOptions, TextRequest,
        TextRunId, TextStyle, TextWrap,
    },
    platform::Platform,
    resource::ImageId,
    style::{Appearance, Clip, Shadow},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeId(u32);

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
    pub text: TextRunId,
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

#[derive(Clone, Copy)]
struct Flow {
    axis: Axis,
    padding: LogicalInsets,
    gap: f32,
    align: Align,
    justify: Justify,
    allow_overflow: bool,
    child_offset: LogicalPoint,
}

#[derive(Clone, Copy, Default)]
struct PayloadId(u32);

#[derive(Clone, Copy, Default)]
struct ContentId(u32);

#[derive(Default)]
pub struct FrameGraph {
    clear: bool,
    nodes: Vec<Node>,
    open: Vec<NodeId>,
    texts: Vec<TextContent>,
    images: Vec<ImageContent>,
    flows: Vec<Flow>,
    appearances: Vec<StoredAppearance>,
    shadows: Vec<Shadow>,
    clip_specs: Vec<Clip>,
    geometry: Vec<GeometryRecord>,
    interactions: Vec<InteractionRecord>,
    gradient_stops: Vec<crate::paint::GradientStop>,
}

struct Node {
    subtree_end: u32,
    item: Item,
    flow: PayloadId,
    content: ContentId,
    appearance: PayloadId,
    shadow: PayloadId,
    clip_spec: PayloadId,
    // layout resolves area in place: dimensions start intrinsic,
    // then each axis writes its final position and size
    area: LogicalRect,
    clip: ClipId,
    clip_bounds: LogicalRect,
}

struct GeometryRecord {
    node: NodeId,
    id: WidgetId,
}

struct InteractionRecord {
    node: NodeId,
    id: WidgetId,
    sense: Sense,
}

struct StoredAppearance {
    background: Color,
    border: StoredBorder,
    radius: BorderRadius,
    opacity: f32,
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

enum ContentRef {
    None,
    Text(usize),
    Image(usize),
}

impl NodeId {
    const ROOT: Self = Self(1);

    fn new(index: usize) -> Self {
        Self(u32::try_from(index + 1).expect("too many nodes in one frame"))
    }

    fn index(self) -> usize {
        self.0.checked_sub(1).expect("missing node") as usize
    }
}

impl PayloadId {
    const NONE: Self = Self(0);

    fn new(index: usize) -> Self {
        Self(u32::try_from(index + 1).expect("too many node payloads in one frame"))
    }

    fn get(self) -> Option<usize> {
        self.0.checked_sub(1).map(|index| index as usize)
    }
}

impl ContentId {
    const IMAGE: u32 = 1 << 31;
    const NONE: Self = Self(0);

    fn text(index: usize) -> Self {
        let id = u32::try_from(index + 1).expect("too much text in one frame");
        assert!(id < Self::IMAGE, "too much text in one frame");
        Self(id)
    }

    fn image(index: usize) -> Self {
        let id = u32::try_from(index + 1).expect("too many images in one frame");
        assert!(id < Self::IMAGE, "too many images in one frame");
        Self(Self::IMAGE | id)
    }

    fn get(self) -> ContentRef {
        match self.0 {
            0 => ContentRef::None,
            id if id & Self::IMAGE == 0 => ContentRef::Text(id as usize - 1),
            id => ContentRef::Image((id & !Self::IMAGE) as usize - 1),
        }
    }
}

impl FrameGraph {
    pub fn begin(&mut self, screen: LogicalRect) {
        self.clear = false;
        self.nodes.clear();
        self.open.clear();
        self.texts.clear();
        self.images.clear();
        self.flows.clear();
        self.appearances.clear();
        self.shadows.clear();
        self.clip_specs.clear();
        self.geometry.clear();
        self.interactions.clear();
        self.gradient_stops.clear();
        self.flows.push(Flow {
            axis: Axis::Vertical,
            padding: LogicalInsets::uniform(0.0),
            gap: 0.0,
            align: Align::Stretch,
            justify: Justify::Start,
            allow_overflow: false,
            child_offset: LogicalPoint::default(),
        });
        self.nodes.push(Node {
            subtree_end: 1,
            item: Item {
                width: Sizing::fixed(screen.width),
                height: Sizing::fixed(screen.height),
            },
            flow: PayloadId::new(0),
            content: ContentId::NONE,
            appearance: PayloadId::NONE,
            shadow: PayloadId::NONE,
            clip_spec: PayloadId::NONE,
            area: screen,
            clip: ClipId::default(),
            clip_bounds: screen,
        });
        self.open.push(NodeId::ROOT);
    }

    pub fn clear(&mut self) {
        self.clear = true;
    }

    pub fn add_container(&mut self, axis: Axis, container: Container<'_>) -> NodeId {
        let node = self.append(
            container.item,
            Some(Flow {
                axis,
                padding: container.padding,
                gap: container.gap,
                align: container.align,
                justify: container.justify,
                allow_overflow: container.allow_overflow,
                child_offset: container.child_offset,
            }),
            container.appearance,
            ContentId::NONE,
            container.clip,
            container.id,
            container.interaction,
        );
        self.open.push(node);
        node
    }

    pub fn add_leaf(&mut self, item: Item, content: Content) -> NodeId {
        let appearance = match content {
            Content::Rectangle(appearance) => appearance,
            _ => Appearance::new(),
        };
        let content = self.store_content(content);
        self.append(item, None, appearance, content, Clip::None, None, None)
    }

    fn append(
        &mut self,
        item: Item,
        flow: Option<Flow>,
        appearance: Appearance<'_>,
        content: ContentId,
        clip: Clip,
        id: Option<WidgetId>,
        interaction: Option<(WidgetId, Sense)>,
    ) -> NodeId {
        self.open.last().expect("node declaration requires a root");
        let node = NodeId::new(self.nodes.len());
        let (appearance, shadow) = self.store_appearance(appearance);
        let flow = flow.map_or(PayloadId::NONE, |flow| {
            let id = PayloadId::new(self.flows.len());
            self.flows.push(flow);
            id
        });
        let clip_spec = if clip == Clip::None {
            PayloadId::NONE
        } else {
            let id = PayloadId::new(self.clip_specs.len());
            self.clip_specs.push(clip);
            id
        };
        self.nodes.push(Node {
            subtree_end: u32::try_from(self.nodes.len() + 1).expect("too many nodes in one frame"),
            item,
            flow,
            appearance,
            shadow,
            content,
            clip_spec,
            area: LogicalRect::default(),
            clip: ClipId::default(),
            clip_bounds: LogicalRect::default(),
        });
        if let Some(id) = id {
            self.geometry.push(GeometryRecord { node, id });
        }
        if let Some((id, sense)) = interaction {
            self.interactions
                .push(InteractionRecord { node, id, sense });
        }
        node
    }

    pub fn set_id(&mut self, node: NodeId, id: WidgetId) {
        self.geometry.push(GeometryRecord { node, id });
    }

    pub fn set_interaction(&mut self, node: NodeId, id: WidgetId, sense: Sense) {
        self.interactions
            .push(InteractionRecord { node, id, sense });
    }

    pub fn set_appearance(&mut self, node: NodeId, appearance: Appearance<'_>) {
        let (appearance, shadow) = self.store_appearance(appearance);
        self.nodes[node.index()].appearance = appearance;
        self.nodes[node.index()].shadow = shadow;
    }

    fn store_content(&mut self, content: Content) -> ContentId {
        match content {
            Content::Rectangle(_) => ContentId::NONE,
            Content::Text(text) => {
                let id = ContentId::text(self.texts.len());
                self.texts.push(text);
                id
            }
            Content::Image(image) => {
                let id = ContentId::image(self.images.len());
                self.images.push(image);
                id
            }
        }
    }

    fn store_appearance(&mut self, appearance: Appearance<'_>) -> (PayloadId, PayloadId) {
        let stored = if appearance.background != Color::TRANSPARENT
            || !matches!(appearance.border, Border::None)
        {
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
            let id = PayloadId::new(self.appearances.len());
            self.appearances.push(StoredAppearance {
                background: appearance.background,
                border,
                radius: appearance.radius,
                opacity: appearance.opacity,
            });
            id
        } else {
            PayloadId::NONE
        };
        let shadow = appearance.shadow.map_or(PayloadId::NONE, |shadow| {
            let id = PayloadId::new(self.shadows.len());
            self.shadows.push(shadow);
            id
        });
        (stored, shadow)
    }

    pub fn close(&mut self, node: NodeId) {
        assert_eq!(self.open.pop(), Some(node), "nodes must close in order");
        let end = u32::try_from(self.nodes.len()).expect("too many nodes in one frame");
        let stored = &mut self.nodes[node.index()];
        stored.subtree_end = end;
        if end as usize == node.index() + 1 && stored.flow.get() == self.flows.len().checked_sub(1)
        {
            self.flows.pop();
            stored.flow = PayloadId::NONE;
        }
    }

    pub fn finish(
        &mut self,
        platform: &mut Platform,
        commands: &mut CommandList,
        interaction: &mut InteractionState,
        geometry: &mut GeometryState,
        scale_factor: f32,
    ) {
        assert_eq!(
            self.open,
            [NodeId::ROOT],
            "a container scope was not dropped"
        );
        self.nodes[0].subtree_end =
            u32::try_from(self.nodes.len()).expect("too many nodes in one frame");
        self.measure_intrinsic(platform);
        self.resolve_axis(Axis::Horizontal);
        self.measure_wrapped_text(platform);
        for index in (1..self.nodes.len()).rev() {
            self.measure_container(NodeId::new(index), Axis::Vertical);
        }
        self.resolve_axis(Axis::Vertical);
        if self.clear {
            commands.push_clear(self.nodes[0].area.to_physical(scale_factor));
        }
        self.resolve_clips(commands);
        self.emit(platform, commands, scale_factor);
        self.register_hits(interaction, scale_factor);
        for record in &self.geometry {
            geometry.register(record.id, self.nodes[record.node.index()].area);
        }
    }

    pub fn memory(&self) -> FrameGraphMemory {
        FrameGraphMemory {
            node_size: size_of::<Node>(),
            node_capacity: self.nodes.capacity(),
            heap_bytes: self.nodes.capacity() * size_of::<Node>()
                + self.open.capacity() * size_of::<NodeId>()
                + self.texts.capacity() * size_of::<TextContent>()
                + self.images.capacity() * size_of::<ImageContent>()
                + self.flows.capacity() * size_of::<Flow>()
                + self.appearances.capacity() * size_of::<StoredAppearance>()
                + self.shadows.capacity() * size_of::<Shadow>()
                + self.clip_specs.capacity() * size_of::<Clip>()
                + self.geometry.capacity() * size_of::<GeometryRecord>()
                + self.interactions.capacity() * size_of::<InteractionRecord>()
                + self.gradient_stops.capacity() * size_of::<crate::paint::GradientStop>(),
        }
    }

    fn flow(&self, node: NodeId) -> Flow {
        self.flows[self.nodes[node.index()]
            .flow
            .get()
            .expect("layout parent has no flow")]
    }

    fn measure_intrinsic(&mut self, platform: &mut Platform) {
        for node in self.nodes.iter_mut().skip(1) {
            let size = match node.content.get() {
                ContentRef::None => LogicalSize::default(),
                ContentRef::Text(index) => {
                    let text = self.texts[index];
                    platform.measure_text(&TextLayoutRequest {
                        text: text.text,
                        style: text.style,
                        wrap: TextWrap::None,
                        max_width: None,
                        max_lines: text.options.max_lines,
                    })
                }
                ContentRef::Image(index) => self.images[index].intrinsic,
            };
            node.area.width = size.width;
            node.area.height = size.height;
        }
        for index in (1..self.nodes.len()).rev() {
            self.measure_container(NodeId::new(index), Axis::Horizontal);
        }
    }

    fn measure_wrapped_text(&mut self, platform: &mut Platform) {
        for node in &mut self.nodes {
            let ContentRef::Text(index) = node.content.get() else {
                continue;
            };
            let text = self.texts[index];
            if text.options.wrap == TextWrap::None {
                continue;
            }
            node.area.height = platform
                .measure_text(&TextLayoutRequest {
                    text: text.text,
                    style: text.style,
                    wrap: text.options.wrap,
                    max_width: Some(node.area.width),
                    max_lines: text.options.max_lines,
                })
                .height;
        }
    }

    fn measure_container(&mut self, id: NodeId, axis: Axis) {
        let mut child = id.index() + 1;
        let end = self.nodes[id.index()].subtree_end as usize;
        if child == end {
            return;
        }
        let layout = self.flow(id);
        let along_flow = layout.axis == axis;
        let mut measured: f32 = 0.0;
        let mut count = 0usize;
        while child < end {
            let node = &self.nodes[child];
            let size = node
                .sizing(axis)
                .resolve(node.size(axis), f32::INFINITY, !along_flow);
            measured = if along_flow {
                measured + size
            } else {
                measured.max(size)
            };
            count += 1;
            child = node.subtree_end as usize;
        }
        if along_flow {
            measured += layout.gap.max(0.0) * count.saturating_sub(1) as f32;
        }
        measured += match axis {
            Axis::Horizontal => layout.padding.left + layout.padding.right,
            Axis::Vertical => layout.padding.top + layout.padding.bottom,
        };
        self.nodes[id.index()].set_size(axis, measured)
    }

    fn resolve_axis(&mut self, axis: Axis) {
        for index in 0..self.nodes.len() {
            if self.nodes[index].flow.get().is_some() {
                self.resolve_children(NodeId::new(index), axis);
            }
        }
    }

    fn resolve_children(&mut self, parent: NodeId, axis: Axis) {
        let layout = self.flow(parent);
        let parent = parent.index();
        let mut child = parent + 1;
        let end = self.nodes[parent].subtree_end as usize;
        let parent_area = self.nodes[parent].area;
        let (origin, available, flow, leading, trailing) = match axis {
            Axis::Horizontal => (
                parent_area.x + layout.child_offset.x,
                parent_area.width,
                layout.axis == Axis::Horizontal,
                layout.padding.left,
                layout.padding.right,
            ),
            Axis::Vertical => (
                parent_area.y + layout.child_offset.y,
                parent_area.height,
                layout.axis == Axis::Vertical,
                layout.padding.top,
                layout.padding.bottom,
            ),
        };
        let available = (available - leading - trailing).max(0.0);

        if !flow {
            while child < end {
                let node = &mut self.nodes[child];
                let sizing = node.sizing(axis);
                let mut size = sizing.resolve(node.size(axis), available, true);
                if layout.align == Align::Stretch && matches!(sizing, Sizing::Fit { .. }) {
                    size = sizing.clamp(available);
                }
                let offset = match layout.align {
                    Align::Start | Align::Stretch => 0.0,
                    Align::Center => (available - size).max(0.0) / 2.0,
                    Align::End => (available - size).max(0.0),
                };
                node.set_axis(axis, origin + leading + offset, size);
                child = node.subtree_end as usize;
            }
            return;
        }

        let mut count = 0usize;
        let mut grow = 0usize;
        let mut used = 0.0;
        while child < end {
            let node = &mut self.nodes[child];
            let sizing = node.sizing(axis);
            let size = sizing.resolve(
                node.size(axis),
                if layout.allow_overflow {
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
            child = node.subtree_end as usize;
        }
        let gaps = layout.gap.max(0.0) * count.saturating_sub(1) as f32;
        let free = available - used - gaps;
        if free < 0.0 && !layout.allow_overflow {
            let mut capacity = 0.0;
            child = parent + 1;
            while child < end {
                let node = &self.nodes[child];
                capacity += node.size(axis) - node.sizing(axis).minimum();
                child = node.subtree_end as usize;
            }
            if capacity > 0.0 {
                let deficit = (-free).min(capacity);
                child = parent + 1;
                while child < end {
                    let node = &mut self.nodes[child];
                    let size = node.size(axis);
                    let available_shrink = size - node.sizing(axis).minimum();
                    let shrunk = size - deficit * available_shrink / capacity;
                    node.set_size(axis, shrunk);
                    used += shrunk - size;
                    child = node.subtree_end as usize;
                }
            }
        }
        let free = free.max(0.0);
        if grow != 0 {
            let share = free / grow as f32;
            child = parent + 1;
            while child < end {
                let node = &mut self.nodes[child];
                if let Sizing::Grow { .. } = node.sizing(axis) {
                    let size = node.size(axis);
                    let grown = node.sizing(axis).clamp(size + share);
                    node.set_size(axis, grown);
                    used += grown - size;
                }
                child = node.subtree_end as usize;
            }
        }

        let remaining = (available - used - gaps).max(0.0);
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
        child = parent + 1;
        while child < end {
            let node = &mut self.nodes[child];
            let size = node.size(axis);
            node.set_axis(axis, cursor, size);
            cursor += size + layout.gap.max(0.0) + extra_gap;
            child = node.subtree_end as usize;
        }
    }

    fn resolve_clips(&mut self, commands: &mut CommandList) {
        self.nodes[0].clip = ClipId::default();
        for index in 0..self.nodes.len() {
            let clip = self.nodes[index]
                .clip_spec
                .get()
                .map_or(Clip::None, |clip| self.clip_specs[clip]);
            let child_clip = match clip {
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
            let child_clip_bounds = match clip {
                Clip::None => self.nodes[index].clip_bounds,
                Clip::Bounds | Clip::Rounded(_) => self.nodes[index]
                    .clip_bounds
                    .intersection(self.nodes[index].area)
                    .unwrap_or_default(),
            };
            let mut child = index + 1;
            let end = self.nodes[index].subtree_end as usize;
            while child < end {
                self.nodes[child].clip = child_clip;
                self.nodes[child].clip_bounds = child_clip_bounds;
                child = self.nodes[child].subtree_end as usize;
            }
        }
    }

    fn emit(&self, platform: &mut Platform, commands: &mut CommandList, scale_factor: f32) {
        for node in self.nodes.iter().skip(1) {
            if let Some(shadow) = node.shadow.get().map(|index| self.shadows[index]) {
                let shadow = BoxShadow::new(node.area, shadow.color)
                    .radius(shadow.radius)
                    .offset(shadow.offset_x, shadow.offset_y)
                    .blur(shadow.blur)
                    .spread(shadow.spread);
                if let Some(bounds) = node.visible_bounds(shadow.bounds(), scale_factor) {
                    commands.push_box_shadow(shadow, bounds, node.clip);
                }
            }
            if let Some(appearance) = node.appearance.get().map(|index| &self.appearances[index]) {
                let border = match appearance.border {
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
                    background: appearance.background,
                    border,
                    radius: appearance.radius,
                    opacity: appearance.opacity,
                };
                if let Some(bounds) = node.visible_bounds(node.area, scale_factor) {
                    commands.push_rectangle(rectangle, bounds, node.clip);
                }
            }
            match node.content.get() {
                ContentRef::None => {}
                ContentRef::Text(index) => {
                    let text = self.texts[index];
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
                            if let Some(bounds) = node.visible_bounds(area, scale_factor) {
                                commands.push_rectangle(
                                    Rectangle::new(area).background(selection.color),
                                    bounds,
                                    node.clip,
                                );
                            }
                        }
                    }
                    if let Some(bounds) = node.visible_bounds(node.area, scale_factor) {
                        commands.push_text(request, bounds, node.clip);
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
                            if let Some(bounds) = node.visible_bounds(area, scale_factor) {
                                commands.push_rectangle(
                                    Rectangle::new(area).background(caret.color),
                                    bounds,
                                    node.clip,
                                );
                            }
                        }
                    }
                }
                ContentRef::Image(index) => {
                    let image = self.images[index];
                    if let Some(bounds) = node.visible_bounds(node.area, scale_factor) {
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
                            node.clip,
                        );
                    }
                }
            }
        }
    }

    fn register_hits(&self, interaction: &mut InteractionState, scale_factor: f32) {
        for record in &self.interactions {
            let node = &self.nodes[record.node.index()];
            let area = node
                .area
                .intersection(node.clip_bounds)
                .map(|area| area.to_physical(scale_factor));
            interaction.register(record.id, area, record.sense);
        }
    }
}

impl Node {
    fn visible_bounds(
        &self,
        area: LogicalRect,
        scale_factor: f32,
    ) -> Option<crate::geometry::PhysicalRect> {
        area.intersection(self.clip_bounds)
            .map(|area| area.to_physical(scale_factor))
    }

    fn sizing(&self, axis: Axis) -> Sizing {
        match axis {
            Axis::Horizontal => self.item.width,
            Axis::Vertical => self.item.height,
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
            Self::Fixed(size) => size.max(0.0),
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

#[derive(Default)]
pub struct GeometryState {
    previous: Vec<(WidgetId, LogicalRect)>,
    current: Vec<(WidgetId, LogicalRect)>,
}

impl GeometryState {
    pub fn get(&self, id: WidgetId) -> Option<LogicalRect> {
        self.previous
            .iter()
            .find_map(|(candidate, geometry)| (*candidate == id).then_some(*geometry))
    }

    pub fn register(&mut self, id: WidgetId, geometry: LogicalRect) {
        self.current.push((id, geometry));
    }

    pub fn end_frame(&mut self) {
        std::mem::swap(&mut self.previous, &mut self.current);
        self.current.clear();
    }
}
