//! frame-local graph construction, layout resolution, and command emission

use crate::{
    FrameGraphMemory,
    color::Color,
    command_list::{BoxShadow, ClipId, CommandList, Rectangle},
    container::{
        Absolute, Align, Anchor, Axis, ContainerConfig, Item, Justify, PositionTarget, Sizing,
    },
    geometry::{LogicalInsets, LogicalPoint, LogicalRect, LogicalSize},
    image::{ImageContent, ImageRequest},
    interact::{InteractionState, WidgetId},
    node::{Content, NodeId},
    renderer::Renderer,
    style::{Border, BorderRadius, Clip, LinearGradient, Shadow, Style},
    text::{TextContent, TextLayoutRequest, TextRequest, TextWrap},
};

#[derive(Default)]
pub struct FrameGraph {
    clear: bool,
    nodes: Vec<Node>,
    open_containers: Vec<NodeId>,
    texts: Vec<TextContent>,
    images: Vec<ImageContent>,
    flows: Vec<Flow>,
    positioned_flows: Vec<PositionedFlow>,
    // absolute subtree roots. finish inserts ROOT to form paint order
    layer_roots: Vec<NodeId>,
    styles: Vec<StoredStyle>,
    shadows: Vec<Shadow>,
    clip_specs: Vec<Clip>,
    geometry: Vec<GeometryRecord>,
    gradient_stops: Vec<crate::style::GradientStop>,
}

impl FrameGraph {
    pub fn begin(&mut self, screen: LogicalRect) {
        self.clear = false;
        self.nodes.clear();
        self.open_containers.clear();
        self.texts.clear();
        self.images.clear();
        self.flows.clear();
        self.positioned_flows.clear();
        self.layer_roots.clear();
        self.styles.clear();
        self.shadows.clear();
        self.clip_specs.clear();
        self.geometry.clear();
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
            flow: FlowId::normal(0),
            content: ContentId::NONE,
            style: StyleId::NONE,
            clip_spec: ClipSpecId::NONE,
            area: screen,
            clip: ClipId::default(),
            clip_bounds: screen,
        });
        self.open_containers.push(NodeId::ROOT);
    }

    pub fn finish(
        &mut self,
        renderer: &mut dyn Renderer,
        commands: &mut CommandList,
        interaction: &mut InteractionState,
        geometry: &mut GeometryState,
        scale_factor: f32,
    ) {
        assert_eq!(
            self.open_containers,
            [NodeId::ROOT],
            "a container scope was not dropped"
        );
        self.nodes[0].subtree_end =
            u32::try_from(self.nodes.len()).expect("too many nodes in one frame");
        if self.positioned_flows.is_empty() {
            self.layout::<false>(renderer);
        } else {
            self.layout::<true>(renderer);
            self.order_layer_roots();
        }
        if self.clear {
            commands.push_clear(self.nodes[0].area.to_physical(scale_factor));
        }
        self.resolve_clips(commands);
        self.emit(renderer, commands, scale_factor);
        self.register_hits(interaction, scale_factor);
        for record in &self.geometry {
            geometry.register(record.id, self.nodes[record.node.index()].area);
        }
    }

    pub fn clear(&mut self) {
        self.clear = true;
    }

    pub fn add_container(
        &mut self,
        axis: Axis,
        container: ContainerConfig<'_>,
        position: Option<Absolute>,
    ) -> NodeId {
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
            position,
            container.style,
            ContentId::NONE,
            container.clip,
            container.id,
        );
        self.open_containers.push(node);
        node
    }

    pub fn add_leaf(&mut self, item: Item, content: Content<'_>) -> NodeId {
        let style = match content {
            Content::Rectangle(style) => style,
            _ => Style::new(),
        };
        let content = self.store_content(content);
        self.append(item, None, None, style, content, Clip::None, None)
    }

    pub fn set_id(&mut self, node: NodeId, id: WidgetId) {
        self.geometry.push(GeometryRecord { node, id });
    }

    pub fn set_style(&mut self, node: NodeId, style: Style<'_>) {
        self.nodes[node.index()].style = self.store_style(style);
    }

    pub fn close(&mut self, node: NodeId) {
        assert_eq!(
            self.open_containers.pop(),
            Some(node),
            "nodes must close in order"
        );
        let end = u32::try_from(self.nodes.len()).expect("too many nodes in one frame");
        let stored = &mut self.nodes[node.index()];
        stored.subtree_end = end;
        if end as usize == node.index() + 1
            && !stored.flow.is_positioned()
            && stored.flow.index() == self.flows.len().checked_sub(1)
        {
            self.flows.pop();
            stored.flow = FlowId::NONE;
        }
    }

    pub fn memory(&self) -> FrameGraphMemory {
        FrameGraphMemory {
            node_size: size_of::<Node>(),
            node_capacity: self.nodes.capacity(),
            heap_bytes: self.nodes.capacity() * size_of::<Node>()
                + self.open_containers.capacity() * size_of::<NodeId>()
                + self.texts.capacity() * size_of::<TextContent>()
                + self.images.capacity() * size_of::<ImageContent>()
                + self.flows.capacity() * size_of::<Flow>()
                + self.positioned_flows.capacity() * size_of::<PositionedFlow>()
                + self.layer_roots.capacity() * size_of::<NodeId>()
                + self.styles.capacity() * size_of::<StoredStyle>()
                + self.shadows.capacity() * size_of::<Shadow>()
                + self.clip_specs.capacity() * size_of::<Clip>()
                + self.geometry.capacity() * size_of::<GeometryRecord>()
                + self.gradient_stops.capacity() * size_of::<crate::style::GradientStop>(),
        }
    }

    fn append(
        &mut self,
        item: Item,
        flow: Option<Flow>,
        position: Option<Absolute>,
        style: Style<'_>,
        content: ContentId,
        clip: Clip,
        id: Option<WidgetId>,
    ) -> NodeId {
        self.open_containers
            .last()
            .expect("node declaration requires a root");
        let node = NodeId::new(self.nodes.len());
        let style = self.store_style(style);
        let flow = match (flow, position) {
            (Some(flow), Some(absolute)) => {
                let id = FlowId::positioned(self.positioned_flows.len());
                self.positioned_flows
                    .push(PositionedFlow { flow, absolute });
                self.layer_roots.push(node);
                id
            }
            (Some(flow), None) => {
                let id = FlowId::normal(self.flows.len());
                self.flows.push(flow);
                id
            }
            (None, None) => FlowId::NONE,
            (None, Some(_)) => unreachable!("only containers can be positioned"),
        };
        let clip_spec = if clip == Clip::None {
            ClipSpecId::NONE
        } else {
            let id = ClipSpecId::new(self.clip_specs.len());
            self.clip_specs.push(clip);
            id
        };
        self.nodes.push(Node {
            subtree_end: u32::try_from(self.nodes.len() + 1).expect("too many nodes in one frame"),
            item,
            flow,
            style,
            content,
            clip_spec,
            area: LogicalRect::default(),
            clip: ClipId::default(),
            clip_bounds: LogicalRect::default(),
        });
        if let Some(id) = id {
            self.geometry.push(GeometryRecord { node, id });
        }
        node
    }

    fn store_content(&mut self, content: Content<'_>) -> ContentId {
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

    fn store_style(&mut self, style: Style<'_>) -> StyleId {
        let shadow = style.shadow.map_or(ShadowId::NONE, |shadow| {
            let id = ShadowId::new(self.shadows.len());
            self.shadows.push(shadow);
            id
        });
        let inset_shadow = style.inset_shadow.map_or(ShadowId::NONE, |shadow| {
            let id = ShadowId::new(self.shadows.len());
            self.shadows.push(shadow);
            id
        });
        if style.background == Color::TRANSPARENT
            && matches!(style.border, Border::None)
            && shadow.index().is_none()
            && inset_shadow.index().is_none()
        {
            return StyleId::NONE;
        }
        let border = match style.border {
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
        let id = StyleId::new(self.styles.len());
        self.styles.push(StoredStyle {
            background: style.background,
            border,
            radius: style.radius,
            opacity: style.opacity,
            shadow,
            inset_shadow,
        });
        id
    }

    fn order_layer_roots(&mut self) {
        let nodes = &self.nodes;
        let positioned = &self.positioned_flows;
        let z_index = |root: &NodeId| {
            positioned[nodes[root.index()].flow.index().unwrap()]
                .absolute
                .z_index
        };
        self.layer_roots
            .sort_unstable_by_key(|root| (z_index(root), root.index()));
        let normal = self.layer_roots.partition_point(|root| z_index(root) < 0);
        self.layer_roots.insert(normal, NodeId::ROOT);

        let layer_roots = &self.layer_roots;
        self.geometry.sort_unstable_by_key(|record| {
            let node = record.node.index();
            let layer = layer_roots
                .iter()
                .enumerate()
                .filter(|(_, layer_root)| {
                    let root = layer_root.index();
                    root <= node && node < nodes[root].subtree_end as usize
                })
                .max_by_key(|(_, layer_root)| layer_root.index())
                .unwrap()
                .0;
            (layer, node)
        });
    }

    fn layout<const POSITIONED: bool>(&mut self, renderer: &mut dyn Renderer) {
        self.measure_intrinsic::<POSITIONED>(renderer);
        self.resolve_axis::<POSITIONED>(Axis::Horizontal);
        self.measure_wrapped_text(renderer);
        for index in (1..self.nodes.len()).rev() {
            self.measure_container::<POSITIONED>(NodeId::new(index), Axis::Vertical);
        }
        self.resolve_axis::<POSITIONED>(Axis::Vertical);
    }

    fn flow<const POSITIONED: bool>(&self, node: NodeId) -> Flow {
        let flow = self.nodes[node.index()].flow;
        let index = flow.index().expect("layout parent has no flow");
        if POSITIONED && flow.is_positioned() {
            self.positioned_flows[index].flow
        } else {
            self.flows[index]
        }
    }

    fn measure_intrinsic<const POSITIONED: bool>(&mut self, renderer: &mut dyn Renderer) {
        for node in self.nodes.iter_mut().skip(1) {
            let size = match node.content.decode() {
                ContentRef::None => LogicalSize::default(),
                ContentRef::Text(index) => {
                    let text = self.texts[index];
                    renderer.measure_text(&TextLayoutRequest {
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
            self.measure_container::<POSITIONED>(NodeId::new(index), Axis::Horizontal);
        }
    }

    fn measure_wrapped_text(&mut self, renderer: &mut dyn Renderer) {
        for node in &mut self.nodes {
            let ContentRef::Text(index) = node.content.decode() else {
                continue;
            };
            let text = self.texts[index];
            if text.options.wrap == TextWrap::None {
                continue;
            }
            node.area.height = renderer
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

    fn measure_container<const POSITIONED: bool>(&mut self, id: NodeId, axis: Axis) {
        let mut child = id.index() + 1;
        let end = self.nodes[id.index()].subtree_end as usize;
        if child == end {
            return;
        }
        let layout = self.flow::<POSITIONED>(id);
        let along_flow = layout.axis == axis;
        let mut measured: f32 = 0.0;
        let mut count = 0usize;
        while child < end {
            let node = &self.nodes[child];
            if !POSITIONED || !node.flow.is_positioned() {
                let size = node
                    .sizing(axis)
                    .resolve(node.size(axis), f32::INFINITY, !along_flow);
                measured = if along_flow {
                    measured + size
                } else {
                    measured.max(size)
                };
                count += 1;
            }
            child = node.subtree_end as usize;
        }
        if count == 0 {
            return;
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

    fn resolve_axis<const POSITIONED: bool>(&mut self, axis: Axis) {
        for index in 0..self.nodes.len() {
            if self.nodes[index].flow.index().is_some() {
                self.resolve_children::<POSITIONED>(NodeId::new(index), axis);
            }
        }
    }

    fn resolve_children<const POSITIONED: bool>(&mut self, parent: NodeId, axis: Axis) {
        let layout = self.flow::<POSITIONED>(parent);
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
                if !POSITIONED || !node.flow.is_positioned() {
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
                }
                child = node.subtree_end as usize;
            }
            if POSITIONED {
                self.resolve_absolute_children(parent, axis);
            }
            return;
        }

        let mut count = 0usize;
        let mut grow = 0usize;
        let mut used = 0.0;
        while child < end {
            let node = &mut self.nodes[child];
            if !POSITIONED || !node.flow.is_positioned() {
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
            }
            child = node.subtree_end as usize;
        }
        let gaps = layout.gap.max(0.0) * count.saturating_sub(1) as f32;
        let free = available - used - gaps;
        if free < 0.0 && !layout.allow_overflow {
            let mut capacity = 0.0;
            child = parent + 1;
            while child < end {
                let node = &self.nodes[child];
                if !POSITIONED || !node.flow.is_positioned() {
                    capacity += node.size(axis) - node.sizing(axis).minimum();
                }
                child = node.subtree_end as usize;
            }
            if capacity > 0.0 {
                let deficit = (-free).min(capacity);
                child = parent + 1;
                while child < end {
                    let node = &mut self.nodes[child];
                    if !POSITIONED || !node.flow.is_positioned() {
                        let size = node.size(axis);
                        let available_shrink = size - node.sizing(axis).minimum();
                        let shrunk = size - deficit * available_shrink / capacity;
                        node.set_size(axis, shrunk);
                        used += shrunk - size;
                    }
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
                if (!POSITIONED || !node.flow.is_positioned())
                    && let Sizing::Grow { .. } = node.sizing(axis)
                {
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
            if !POSITIONED || !node.flow.is_positioned() {
                let size = node.size(axis);
                node.set_axis(axis, cursor, size);
                cursor += size + layout.gap.max(0.0) + extra_gap;
            }
            child = node.subtree_end as usize;
        }
        if POSITIONED {
            self.resolve_absolute_children(parent, axis);
        }
    }

    fn resolve_absolute_children(&mut self, parent: usize, axis: Axis) {
        let mut child = parent + 1;
        let end = self.nodes[parent].subtree_end as usize;
        while child < end {
            let node = &self.nodes[child];
            let next = node.subtree_end as usize;
            if !node.flow.is_positioned() {
                child = next;
                continue;
            }
            let position = self.positioned_flows[node.flow.index().unwrap()].absolute;
            let target = match position.target {
                PositionTarget::Parent => self.nodes[parent].area,
                PositionTarget::Screen => self.nodes[0].area,
            };
            let (origin, available, offset) = match axis {
                Axis::Horizontal => (target.x, target.width, position.offset.x),
                Axis::Vertical => (target.y, target.height, position.offset.y),
            };
            let node = &mut self.nodes[child];
            let size = node.sizing(axis).resolve(node.size(axis), available, true);
            let position = origin + available * position.target_anchor.factor(axis)
                - size * position.child_anchor.factor(axis)
                + offset;
            node.set_axis(axis, position, size);
            child = next;
        }
    }

    fn resolve_clips(&mut self, commands: &mut CommandList) {
        self.nodes[0].clip = ClipId::default();
        for index in 0..self.nodes.len() {
            let clip = self.nodes[index]
                .clip_spec
                .index()
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

    fn emit(&self, renderer: &mut dyn Renderer, commands: &mut CommandList, scale_factor: f32) {
        if self.layer_roots.is_empty() {
            for index in 1..self.nodes.len() {
                self.emit_node(index, renderer, commands, scale_factor);
            }
            return;
        }

        for root in &self.layer_roots {
            self.emit_layer(root.index(), renderer, commands, scale_factor);
        }
    }

    fn emit_layer(
        &self,
        root: usize,
        renderer: &mut dyn Renderer,
        commands: &mut CommandList,
        scale_factor: f32,
    ) {
        let mut index = root.max(1);
        let end = if root == 0 {
            self.nodes.len()
        } else {
            self.nodes[root].subtree_end as usize
        };
        while index < end {
            let node = &self.nodes[index];
            if index != root && node.flow.is_positioned() {
                index = node.subtree_end as usize;
                continue;
            }
            self.emit_node(index, renderer, commands, scale_factor);
            index += 1;
        }
    }

    #[inline(always)]
    fn emit_node(
        &self,
        index: usize,
        renderer: &mut dyn Renderer,
        commands: &mut CommandList,
        scale_factor: f32,
    ) {
        let node = &self.nodes[index];
        if let Some(style) = node.style.index().map(|index| &self.styles[index]) {
            if let Some(shadow) = style.shadow.index().map(|index| self.shadows[index]) {
                let shadow = BoxShadow::new(node.area, shadow.color)
                    .radius(style.radius)
                    .offset(shadow.offset_x, shadow.offset_y)
                    .blur(shadow.blur)
                    .spread(shadow.spread);
                if let Some(bounds) = node.visible_bounds(shadow.bounds(), scale_factor) {
                    commands.push_box_shadow(shadow, bounds, node.clip);
                }
            }
            if style.background != Color::TRANSPARENT || !matches!(style.border, StoredBorder::None)
            {
                let border = match style.border {
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
                    background: style.background,
                    border,
                    radius: style.radius,
                    opacity: style.opacity,
                };
                if let Some(bounds) = node.visible_bounds(node.area, scale_factor) {
                    commands.push_rectangle(rectangle, bounds, node.clip);
                }
            }
            if let Some(shadow) = style.inset_shadow.index().map(|index| self.shadows[index]) {
                let shadow = BoxShadow::new(node.area, shadow.color)
                    .radius(style.radius)
                    .offset(shadow.offset_x, shadow.offset_y)
                    .blur(shadow.blur)
                    .spread(shadow.spread)
                    .inset(true);
                if let Some(bounds) = node.visible_bounds(shadow.bounds(), scale_factor) {
                    commands.push_box_shadow(shadow, bounds, node.clip);
                }
            }
        }
        match node.content.decode() {
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
                    let start = renderer.text_cursor_rect(&request, selection.start);
                    let end = renderer.text_cursor_rect(&request, selection.end);
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
                    let cursor = renderer.text_cursor_rect(&request, caret.offset);
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

    fn register_hits(&self, interaction: &mut InteractionState, scale_factor: f32) {
        interaction.register_hits(self.geometry.iter().map(|record| {
            let node = &self.nodes[record.node.index()];
            let area = node
                .area
                .intersection(node.clip_bounds)
                .map(|area| area.to_physical(scale_factor));
            (record.id, area)
        }));
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

struct Node {
    subtree_end: u32,
    item: Item,
    flow: FlowId,
    content: ContentId,
    style: StyleId,
    clip_spec: ClipSpecId,
    // layout resolves area in place: dimensions start intrinsic,
    // then each axis writes its final position and size
    area: LogicalRect,
    clip: ClipId,
    clip_bounds: LogicalRect,
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

struct PositionedFlow {
    flow: Flow,
    absolute: Absolute,
}

impl Anchor {
    fn factor(self, axis: Axis) -> f32 {
        match (axis, self) {
            (Axis::Horizontal, Self::TopLeft | Self::Left | Self::BottomLeft)
            | (Axis::Vertical, Self::TopLeft | Self::Top | Self::TopRight) => 0.0,
            (Axis::Horizontal, Self::Top | Self::Center | Self::Bottom)
            | (Axis::Vertical, Self::Left | Self::Center | Self::Right) => 0.5,
            (Axis::Horizontal, Self::TopRight | Self::Right | Self::BottomRight)
            | (Axis::Vertical, Self::BottomLeft | Self::Bottom | Self::BottomRight) => 1.0,
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

struct GeometryRecord {
    node: NodeId,
    id: WidgetId,
}

struct StoredStyle {
    background: Color,
    border: StoredBorder,
    radius: BorderRadius,
    opacity: f32,
    shadow: ShadowId,
    inset_shadow: ShadowId,
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

/// index into `flows`, or into `positioned_flows` when the high bit is set
#[derive(Clone, Copy, Default)]
struct FlowId(u32);

impl FlowId {
    const POSITIONED: u32 = 1 << 31;
    const NONE: Self = Self(0);

    fn normal(index: usize) -> Self {
        let id = u32::try_from(index + 1).expect("too many flows in one frame");
        assert!(id < Self::POSITIONED, "too many flows in one frame");
        Self(id)
    }

    fn positioned(index: usize) -> Self {
        let id = u32::try_from(index + 1).expect("too many positioned flows in one frame");
        assert!(
            id < Self::POSITIONED,
            "too many positioned flows in one frame"
        );
        Self(Self::POSITIONED | id)
    }

    fn index(self) -> Option<usize> {
        (self.0 & !Self::POSITIONED)
            .checked_sub(1)
            .map(|index| index as usize)
    }

    fn is_positioned(self) -> bool {
        self.0 & Self::POSITIONED != 0
    }
}

/// index into `texts`, or into `images` when the high bit is set
#[derive(Clone, Copy, Default)]
struct ContentId(u32);

enum ContentRef {
    None,
    Text(usize),
    Image(usize),
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

    fn decode(self) -> ContentRef {
        match self.0 {
            0 => ContentRef::None,
            id if id & Self::IMAGE == 0 => ContentRef::Text(id as usize - 1),
            id => ContentRef::Image((id & !Self::IMAGE) as usize - 1),
        }
    }
}

macro_rules! store_ids {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Copy, Default)]
            struct $name(u32);

            impl $name {
                const NONE: Self = Self(0);

                fn new(index: usize) -> Self {
                    Self(u32::try_from(index + 1).expect("too many stored values in one frame"))
                }

                fn index(self) -> Option<usize> {
                    self.0.checked_sub(1).map(|index| index as usize)
                }
            }
        )+
    };
}

store_ids!(StyleId, ShadowId, ClipSpecId);
