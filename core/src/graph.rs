use crate::{
    FrameGraphMemory,
    color::Color,
    command_list::{ClipId, CommandList},
    element::{
        Align, Appearance, Axis, Clip, Content, ElementGeometry, Flow, ImageContent, Item, Justify,
        NodeSpec, Shadow, Sizing, TextContent,
    },
    geometry::{LogicalPoint, LogicalRect, LogicalSize},
    interact::{InteractionState, Sense, WidgetId},
    paint::{
        Border, BorderRadius, BoxShadow, ImageRequest, LinearGradient, Rectangle,
        TextLayoutRequest, TextRequest, TextWrap,
    },
    platform::Platform,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeId(u32);

#[derive(Clone, Copy, Default)]
struct PayloadId(u32);

#[derive(Clone, Copy, Default)]
struct ContentId(u32);

#[derive(Default)]
pub struct FrameGraph {
    nodes: Vec<Node>,
    open: Vec<NodeId>,
    scratch: Vec<NodeId>,
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
    offset: LogicalPoint,
    intrinsic: LogicalSize,
    area: LogicalRect,
    clip: ClipId,
    content_clip: ClipId,
    clip_bounds: LogicalRect,
    content_clip_bounds: LogicalRect,
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
    replace: bool,
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
    Rectangle,
    Text(usize),
    Image(usize),
}

impl NodeId {
    const ROOT: Self = Self(1);

    fn new(index: usize) -> Self {
        Self(u32::try_from(index + 1).expect("too many elements in one frame"))
    }

    fn index(self) -> usize {
        self.0.checked_sub(1).expect("missing element") as usize
    }
}

impl PayloadId {
    const NONE: Self = Self(0);

    fn new(index: usize) -> Self {
        Self(u32::try_from(index + 1).expect("too many element payloads in one frame"))
    }

    fn get(self) -> Option<usize> {
        self.0.checked_sub(1).map(|index| index as usize)
    }
}

impl ContentId {
    const IMAGE: u32 = 1 << 31;
    const NONE: Self = Self(0);
    const RECTANGLE: Self = Self(1);

    fn text(index: usize) -> Self {
        let id = u32::try_from(index + 2).expect("too much text in one frame");
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
            1 => ContentRef::Rectangle,
            id if id & Self::IMAGE == 0 => ContentRef::Text(id as usize - 2),
            id => ContentRef::Image((id & !Self::IMAGE) as usize - 1),
        }
    }
}

impl FrameGraph {
    pub fn begin(&mut self, screen: LogicalRect) {
        self.nodes.clear();
        self.open.clear();
        self.scratch.clear();
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
            padding: crate::geometry::LogicalInsets::uniform(0.0),
            gap: 0.0,
            align: Align::Stretch,
            justify: Justify::Start,
            overflow: false,
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
            offset: LogicalPoint::default(),
            intrinsic: LogicalSize::default(),
            area: screen,
            clip: ClipId::default(),
            content_clip: ClipId::default(),
            clip_bounds: screen,
            content_clip_bounds: screen,
        });
        self.open.push(NodeId::ROOT);
    }

    pub fn push(&mut self, spec: NodeSpec<'_>) -> NodeId {
        let node = self.append(spec);
        self.open.push(node);
        node
    }

    pub fn push_leaf(&mut self, spec: NodeSpec<'_>) {
        self.append(spec);
    }

    fn append(&mut self, spec: NodeSpec<'_>) -> NodeId {
        self.open
            .last()
            .expect("element declaration requires a root");
        let node = NodeId::new(self.nodes.len());
        let content = self.store_content(spec.content);
        let (appearance, shadow) = self.store_appearance(spec.appearance);
        let flow = spec.flow.map_or(PayloadId::NONE, |flow| {
            let id = PayloadId::new(self.flows.len());
            self.flows.push(flow);
            id
        });
        let clip_spec = match spec.clip {
            Clip::None => PayloadId::NONE,
            clip => {
                let id = PayloadId::new(self.clip_specs.len());
                self.clip_specs.push(clip);
                id
            }
        };
        self.nodes.push(Node {
            subtree_end: u32::try_from(self.nodes.len() + 1)
                .expect("too many elements in one frame"),
            item: spec.item,
            flow,
            appearance,
            shadow,
            content,
            clip_spec,
            offset: spec.offset,
            intrinsic: LogicalSize::default(),
            area: LogicalRect::default(),
            clip: ClipId::default(),
            content_clip: ClipId::default(),
            clip_bounds: LogicalRect::default(),
            content_clip_bounds: LogicalRect::default(),
        });
        if let Some(id) = spec.id {
            self.geometry.push(GeometryRecord { node, id });
        }
        if let Some((id, sense)) = spec.interaction {
            self.interactions
                .push(InteractionRecord { node, id, sense });
        }
        node
    }

    pub fn set_appearance(&mut self, node: NodeId, appearance: Appearance<'_>) {
        let (appearance, shadow) = self.store_appearance(appearance);
        self.nodes[node.index()].appearance = appearance;
        self.nodes[node.index()].shadow = shadow;
    }

    fn store_content(&mut self, content: Content) -> ContentId {
        match content {
            Content::None => ContentId::NONE,
            Content::Rectangle => ContentId::RECTANGLE,
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
            || appearance.replace
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
                replace: appearance.replace,
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
        assert_eq!(self.open.pop(), Some(node), "elements must close in order");
        let end = u32::try_from(self.nodes.len()).expect("too many elements in one frame");
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
            "an element scope was not dropped"
        );
        self.nodes[0].subtree_end =
            u32::try_from(self.nodes.len()).expect("too many elements in one frame");
        self.measure_intrinsic(platform);
        self.resolve_axis(Axis::Horizontal);
        self.measure_wrapped_text(platform);
        self.measure_container_heights();
        self.resolve_axis(Axis::Vertical);
        self.resolve_clips(commands);
        self.emit(platform, commands, scale_factor);
        self.register_hits(interaction, scale_factor);
        for record in &self.geometry {
            geometry.register(
                record.id,
                ElementGeometry {
                    area: self.nodes[record.node.index()].area,
                },
            );
        }
        self.nodes.clear();
        self.open.clear();
        self.scratch.clear();
        self.texts.clear();
        self.images.clear();
        self.flows.clear();
        self.appearances.clear();
        self.shadows.clear();
        self.clip_specs.clear();
        self.geometry.clear();
        self.interactions.clear();
        self.gradient_stops.clear();
    }

    pub fn memory(&self) -> FrameGraphMemory {
        FrameGraphMemory {
            node_size: size_of::<Node>(),
            node_capacity: self.nodes.capacity(),
            heap_bytes: self.nodes.capacity() * size_of::<Node>()
                + self.open.capacity() * size_of::<NodeId>()
                + self.scratch.capacity() * size_of::<NodeId>()
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

    fn first_child(&self, parent: NodeId) -> Option<NodeId> {
        let index = parent.index() + 1;
        (index < self.nodes[parent.index()].subtree_end as usize).then(|| NodeId::new(index))
    }

    fn next_sibling(&self, parent: NodeId, child: NodeId) -> Option<NodeId> {
        let index = self.nodes[child.index()].subtree_end as usize;
        (index < self.nodes[parent.index()].subtree_end as usize).then(|| NodeId::new(index))
    }

    fn has_children(&self, node: NodeId) -> bool {
        node.index() + 1 < self.nodes[node.index()].subtree_end as usize
    }

    fn flow(&self, node: NodeId) -> Flow {
        self.flows[self.nodes[node.index()]
            .flow
            .get()
            .expect("layout parent has no flow")]
    }

    fn measure_intrinsic(&mut self, platform: &mut Platform) {
        for node in &mut self.nodes {
            node.intrinsic = match node.content.get() {
                ContentRef::None | ContentRef::Rectangle => LogicalSize::default(),
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
        }
        for index in (0..self.nodes.len()).rev() {
            self.measure_container(NodeId::new(index), true);
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
            node.intrinsic = platform.measure_text(&TextLayoutRequest {
                text: text.text,
                style: text.style,
                wrap: text.options.wrap,
                max_width: Some(node.area.width),
                max_lines: text.options.max_lines,
            });
        }
    }

    fn measure_container_heights(&mut self) {
        for index in (0..self.nodes.len()).rev() {
            self.measure_container(NodeId::new(index), false);
        }
    }

    fn measure_container(&mut self, id: NodeId, width: bool) {
        let Some(mut child) = self.first_child(id) else {
            return;
        };
        let layout = self.flow(id);
        let mut main: f32 = 0.0;
        let mut cross: f32 = 0.0;
        let mut count = 0usize;
        loop {
            let node = &self.nodes[child.index()];
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
            let Some(next) = self.next_sibling(id, child) else {
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
            self.nodes[id.index()].intrinsic.width = measured.width;
        } else {
            self.nodes[id.index()].intrinsic.height = measured.height;
        }
    }

    fn resolve_axis(&mut self, axis: Axis) {
        self.scratch.clear();
        self.scratch.push(NodeId::ROOT);
        while let Some(parent) = self.scratch.pop() {
            self.resolve_children(parent, axis);
            let mut child = self.first_child(parent);
            while let Some(id) = child {
                if self.has_children(id) {
                    self.scratch.push(id);
                }
                child = self.next_sibling(parent, id);
            }
        }
    }

    fn resolve_children(&mut self, parent: NodeId, axis: Axis) {
        let layout = self.flow(parent);
        let Some(mut child) = self.first_child(parent) else {
            return;
        };
        let parent_area = self.nodes[parent.index()].area;
        let (origin, available, flow, leading, trailing) = match axis {
            Axis::Horizontal => (
                parent_area.x + self.nodes[parent.index()].offset.x,
                parent_area.width,
                layout.axis == Axis::Horizontal,
                layout.padding.left,
                layout.padding.right,
            ),
            Axis::Vertical => (
                parent_area.y + self.nodes[parent.index()].offset.y,
                parent_area.height,
                layout.axis == Axis::Vertical,
                layout.padding.top,
                layout.padding.bottom,
            ),
        };
        let available = (available - leading - trailing).max(0.0);

        if !flow {
            loop {
                let sizing = self.nodes[child.index()].sizing(axis);
                let intrinsic = self.nodes[child.index()].intrinsic(axis);
                let mut size = sizing.resolve(intrinsic, available, true);
                if layout.align == Align::Stretch && matches!(sizing, Sizing::Fit { .. }) {
                    size = sizing.clamp(available);
                }
                let offset = match layout.align {
                    Align::Start | Align::Stretch => 0.0,
                    Align::Center => (available - size).max(0.0) / 2.0,
                    Align::End => (available - size).max(0.0),
                };
                self.nodes[child.index()].set_axis(axis, origin + leading + offset, size);
                let Some(next) = self.next_sibling(parent, child) else {
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
            let node = &mut self.nodes[child.index()];
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
            let Some(next) = self.next_sibling(parent, child) else {
                break;
            };
            child = next;
        }
        let gaps = layout.gap.max(0.0) * count.saturating_sub(1) as f32;
        let free = available - used - gaps;
        if free < 0.0 && !layout.overflow {
            let mut capacity = 0.0;
            child = self.first_child(parent).unwrap();
            loop {
                let node = &self.nodes[child.index()];
                capacity += node.size(axis) - node.sizing(axis).minimum();
                let Some(next) = self.next_sibling(parent, child) else {
                    break;
                };
                child = next;
            }
            if capacity > 0.0 {
                let deficit = (-free).min(capacity);
                child = self.first_child(parent).unwrap();
                loop {
                    let node = &mut self.nodes[child.index()];
                    let available_shrink = node.size(axis) - node.sizing(axis).minimum();
                    node.set_size(
                        axis,
                        node.size(axis) - deficit * available_shrink / capacity,
                    );
                    let Some(next) = self.next_sibling(parent, child) else {
                        break;
                    };
                    child = next;
                }
            }
        }
        let free = free.max(0.0);
        if grow != 0 {
            let share = free / grow as f32;
            child = self.first_child(parent).unwrap();
            loop {
                let node = &mut self.nodes[child.index()];
                if let Sizing::Grow { .. } = node.sizing(axis) {
                    node.set_size(axis, node.sizing(axis).clamp(node.size(axis) + share));
                }
                let Some(next) = self.next_sibling(parent, child) else {
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
        child = self.first_child(parent).unwrap();
        loop {
            let size = self.nodes[child.index()].size(axis);
            self.nodes[child.index()].set_axis(axis, cursor, size);
            cursor += size + layout.gap.max(0.0) + extra_gap;
            let Some(next) = self.next_sibling(parent, child) else {
                break;
            };
            child = next;
        }
    }

    fn children_size(&self, parent: NodeId, axis: Axis) -> f32 {
        let mut total = 0.0;
        let mut child = self.first_child(parent);
        while let Some(id) = child {
            total += self.nodes[id.index()].size(axis);
            child = self.next_sibling(parent, id);
        }
        total
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
            self.nodes[index].content_clip = child_clip;
            self.nodes[index].content_clip_bounds = match clip {
                Clip::None => self.nodes[index].clip_bounds,
                Clip::Bounds | Clip::Rounded(_) => self.nodes[index]
                    .clip_bounds
                    .intersection(self.nodes[index].area)
                    .unwrap_or_default(),
            };
            let parent = NodeId::new(index);
            let mut child = self.first_child(parent);
            while let Some(child_id) = child {
                self.nodes[child_id.index()].clip = child_clip;
                self.nodes[child_id.index()].clip_bounds = self.nodes[index].content_clip_bounds;
                child = self.next_sibling(parent, child_id);
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
                if let Some(bounds) = node.visible_bounds(shadow.bounds(), false, scale_factor) {
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
                    replace: appearance.replace,
                };
                if let Some(bounds) = node.visible_bounds(node.area, false, scale_factor) {
                    commands.push_rectangle(rectangle, bounds, node.clip);
                }
            }
            match node.content.get() {
                ContentRef::None | ContentRef::Rectangle => {}
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
                ContentRef::Image(index) => {
                    let image = self.images[index];
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
            Axis::Horizontal => self.item.width,
            Axis::Vertical => self.item.height,
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
