//! frame-local graph construction, layout resolution, and command emission

use std::{
    alloc::{Layout as AllocationLayout, alloc, dealloc, handle_alloc_error},
    any::TypeId,
    mem::{align_of, needs_drop, size_of},
    ptr::NonNull,
    time::Duration,
};

use crate::{
    FrameGraphMemory,
    animation::{Transition, TransitionProperties},
    color::Color,
    command_list::{BoxShadow, ClipId, CommandList, Rectangle},
    container::{Absolute, Anchor, ContainerConfig, Item, PositionTarget, Sizing},
    geometry::{LogicalPoint, LogicalRect, LogicalSize},
    image::{ImageContent, ImageRequest},
    interact::{InteractionState, WidgetId},
    layout::{Axis, Flex, Layout},
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
    layouts: Vec<StoredLayout>,
    positioned_layouts: Vec<PositionedLayout>,
    layout_nodes: Vec<NodeId>,
    layout_data: DataArena,
    position_offsets: Vec<LogicalPoint>,
    // absolute subtree roots. finish inserts ROOT to form paint order
    layer_roots: Vec<NodeId>,
    styles: Vec<StoredStyle>,
    shadows: Vec<Shadow>,
    clip_specs: Vec<Clip>,
    geometry: Vec<GeometryRecord>,
    gradient_stops: Vec<crate::style::GradientStop>,
}

/// access to one container and its direct children during layout
pub struct LayoutCx<'a, I> {
    frame: &'a mut FrameGraph,
    nodes: *const Node,
    node: NodeId,
    positioned: bool,
    item: std::marker::PhantomData<fn() -> I>,
    offset: LogicalPoint,
}

/// iterator over a layout container's direct children
pub struct Children<'a> {
    nodes: *const Node,
    next: usize,
    end: usize,
    marker: std::marker::PhantomData<&'a ()>,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.end {
            return None;
        }
        let node = NodeId::new(self.next);
        // safety: layout freezes node storage and subtree boundaries
        self.next = unsafe { (*self.nodes.add(self.next)).subtree_end as usize };
        Some(node)
    }
}

impl<'a, I: Copy + 'static> LayoutCx<'a, I> {
    /// the container being laid out
    #[inline]
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// the container's current rectangle
    #[inline]
    pub fn rect(&self) -> LogicalRect {
        self.frame.nodes[self.node.index()].area
    }

    /// the current size of a node
    #[inline]
    pub fn size(&self, node: NodeId) -> LogicalSize {
        let area = self.frame.nodes[node.index()].area;
        LogicalSize {
            width: area.width,
            height: area.height,
        }
    }

    /// iterates over this container's direct children
    #[inline]
    pub fn children(&self) -> Children<'a> {
        Children {
            nodes: self.nodes,
            next: self.node.index() + 1,
            end: self.frame.nodes[self.node.index()].subtree_end as usize,
            marker: std::marker::PhantomData,
        }
    }

    /// whether a child participates in this container's layout
    #[inline]
    pub fn is_in_flow(&self, node: NodeId) -> bool {
        !self.positioned || !self.frame.nodes[node.index()].layout.is_positioned()
    }

    /// metadata supplied by a direct child for this layout
    ///
    /// panics when the child was declared without metadata
    #[inline]
    pub fn item(&self, node: NodeId) -> I {
        assert_eq!(
            self.frame.nodes[node.index()].parent,
            self.node,
            "layout metadata is only available for direct children"
        );
        let offset = self.frame.nodes[node.index()]
            .layout_item
            .offset()
            .expect("layout item is missing. unit scope does not store metadata");
        debug_assert!(offset + size_of::<I>() <= self.frame.layout_data.len());
        // safety: the parent scope stored an aligned I for this direct child
        unsafe {
            self.frame
                .layout_data
                .as_ptr()
                .add(offset)
                .cast::<I>()
                .read()
        }
    }

    /// the sizing requested by a child on an axis
    #[inline]
    pub fn sizing(&self, node: NodeId, axis: Axis) -> crate::container::Sizing {
        self.frame.nodes[node.index()].sizing(axis)
    }

    /// the current size of a child on an axis
    #[inline]
    pub fn axis_size(&self, node: NodeId, axis: Axis) -> f32 {
        self.frame.nodes[node.index()].size(axis)
    }

    /// sets a child's size on an axis without changing its position
    #[inline]
    pub fn set_size(&mut self, node: NodeId, axis: Axis, size: f32) {
        self.frame.nodes[node.index()].set_size(axis, size)
    }

    /// sets a child's position and size on an axis
    #[inline]
    pub fn set_axis(&mut self, node: NodeId, axis: Axis, position: f32, size: f32) {
        let position = position
            + match axis {
                Axis::Horizontal => self.offset.x,
                Axis::Vertical => self.offset.y,
            };
        self.frame.nodes[node.index()].set_axis(axis, position, size)
    }
}

impl FrameGraph {
    pub fn begin(&mut self, screen: LogicalRect) {
        self.clear = false;
        self.nodes.clear();
        self.open_containers.clear();
        self.texts.clear();
        self.images.clear();
        self.layouts.clear();
        self.positioned_layouts.clear();
        self.layout_nodes.clear();
        self.layout_data.clear();
        self.layer_roots.clear();
        self.styles.clear();
        self.shadows.clear();
        self.clip_specs.clear();
        self.geometry.clear();
        self.gradient_stops.clear();
        let root = self.store_layout(Flex::column(), LogicalPoint::default());
        self.layouts.push(root);
        self.layout_nodes.push(NodeId::ROOT);
        let root_layout = LayoutId::normal(0);
        self.nodes.push(Node {
            parent: NodeId::ROOT,
            subtree_end: 1,
            item: Item {
                width: Sizing::fixed(screen.width),
                height: Sizing::fixed(screen.height),
            },
            layout: root_layout,
            layout_item: LayoutItemId::NONE,
            content: ContentId::NONE,
            style: StyleId::NONE,
            clip_spec: ClipSpecId::NONE,
            area: screen,
            clip: ClipId::default(),
            clip_bounds: screen,
        });
        self.open_containers.push(NodeId::ROOT);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finish(
        &mut self,
        renderer: &mut dyn Renderer,
        commands: &mut CommandList,
        interaction: &mut InteractionState,
        geometry: &mut GeometryState,
        transition_states: &mut [TransitionState],
        time: Duration,
        scale_factor: f32,
    ) {
        assert_eq!(
            self.open_containers,
            [NodeId::ROOT],
            "a container scope was not dropped"
        );
        self.nodes[0].subtree_end =
            u32::try_from(self.nodes.len()).expect("too many nodes in one frame");
        let positioned = !self.positioned_layouts.is_empty();
        if positioned {
            self.layout::<true>(renderer);
        } else {
            self.layout::<false>(renderer);
        }
        let mut active_transitions = TransitionProperties::NONE;
        let mut has_position_transition = false;
        for state in transition_states.iter_mut().filter(|state| state.seen) {
            has_position_transition |= state
                .config
                .properties
                .contains(TransitionProperties::POSITION);
            state.advance(self.transition_area(state.node, state.parent), time);
            active_transitions = active_transitions.union(state.active);
        }
        if has_position_transition {
            self.position_offsets
                .resize(self.nodes.len(), LogicalPoint::default());
        }
        if active_transitions.contains(TransitionProperties::SIZE) {
            self.prepare_transitioned_dimensions(transition_states);
            if positioned {
                self.layout::<true>(renderer);
            } else {
                self.layout::<false>(renderer);
            }
        }
        if active_transitions.contains(TransitionProperties::POSITION) {
            self.apply_transitioned_positions(transition_states);
        }
        if positioned {
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
        self.layout_data.clear();
    }

    pub fn clear(&mut self) {
        self.clear = true;
    }

    pub fn add_container<L: Layout>(
        &mut self,
        layout: L,
        container: ContainerConfig<'_>,
    ) -> NodeId {
        let layout = self.store_layout(layout, container.offset);
        let node = self.append(
            container.item,
            Some(layout),
            container.absolute,
            container.style,
            ContentId::NONE,
            container.clip,
            container.id,
        );
        self.open_containers.push(node);
        self.layout_nodes.push(node);
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

    pub fn transition_parent(&self, node: NodeId) -> NodeId {
        if self.open_containers.last() == Some(&node) {
            self.open_containers[self.open_containers.len() - 2]
        } else {
            *self
                .open_containers
                .last()
                .expect("transition requires a layout parent")
        }
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
            && !stored.layout.is_positioned()
            && stored.layout.index() == self.layouts.len().checked_sub(1)
        {
            self.layouts.pop();
            let _last = self.layout_nodes.pop();
            debug_assert_eq!(_last, Some(node));
            stored.layout = LayoutId::NONE;
        }
    }

    pub fn begin_layout_item(&self) -> NodeId {
        NodeId::new(self.nodes.len())
    }

    pub fn finish_layout_item<L: Layout>(&mut self, parent: NodeId, child: NodeId, item: L::Item) {
        assert_eq!(
            self.open_containers.last(),
            Some(&parent),
            "layout item child scopes must be closed before returning"
        );
        assert!(
            child.index() < self.nodes.len()
                && self.nodes[child.index()].parent == parent
                && self.nodes[child.index()].subtree_end as usize == self.nodes.len(),
            "a layout item must declare exactly one child subtree"
        );
        let layout = self
            .stored_layout(parent)
            .expect("layout item parent must have a layout");
        assert_eq!(
            (layout.vtable.layout_type)(),
            TypeId::of::<L>(),
            "layout item type does not match its parent layout"
        );
        let offset = self.store_data(item);
        self.nodes[child.index()].layout_item = LayoutItemId::new(offset);
    }

    pub fn memory(&self) -> FrameGraphMemory {
        FrameGraphMemory {
            node_size: size_of::<Node>(),
            node_capacity: self.nodes.capacity(),
            heap_bytes: self.nodes.capacity() * size_of::<Node>()
                + self.open_containers.capacity() * size_of::<NodeId>()
                + self.texts.capacity() * size_of::<TextContent>()
                + self.images.capacity() * size_of::<ImageContent>()
                + self.layouts.capacity() * size_of::<StoredLayout>()
                + self.positioned_layouts.capacity() * size_of::<PositionedLayout>()
                + self.layout_nodes.capacity() * size_of::<NodeId>()
                + self.layout_data.heap_bytes()
                + self.position_offsets.capacity() * size_of::<LogicalPoint>()
                + self.layer_roots.capacity() * size_of::<NodeId>()
                + self.styles.capacity() * size_of::<StoredStyle>()
                + self.shadows.capacity() * size_of::<Shadow>()
                + self.clip_specs.capacity() * size_of::<Clip>()
                + self.geometry.capacity() * size_of::<GeometryRecord>()
                + self.gradient_stops.capacity() * size_of::<crate::style::GradientStop>(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn append(
        &mut self,
        item: Item,
        layout: Option<StoredLayout>,
        position: Option<Absolute>,
        style: Style<'_>,
        content: ContentId,
        clip: Clip,
        id: Option<WidgetId>,
    ) -> NodeId {
        let parent = *self
            .open_containers
            .last()
            .expect("node declaration requires a root");
        let node = NodeId::new(self.nodes.len());
        let style = self.store_style(style);
        let layout = match (layout, position) {
            (Some(layout), Some(absolute)) => {
                let id = LayoutId::positioned(self.positioned_layouts.len());
                self.positioned_layouts
                    .push(PositionedLayout { layout, absolute });
                self.layer_roots.push(node);
                id
            }
            (Some(layout), None) => {
                let id = LayoutId::normal(self.layouts.len());
                self.layouts.push(layout);
                id
            }
            (None, None) => LayoutId::NONE,
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
            parent,
            subtree_end: u32::try_from(self.nodes.len() + 1).expect("too many nodes in one frame"),
            item,
            layout,
            layout_item: LayoutItemId::NONE,
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

    fn store_layout<L: Layout>(&mut self, layout: L, offset: LogicalPoint) -> StoredLayout {
        let data_offset = self.store_data(layout);
        StoredLayout {
            data_offset,
            vtable: &LayoutVtableFor::<L>::VALUE,
            offset,
        }
    }

    fn store_data<T>(&mut self, value: T) -> u32 {
        self.layout_data.store(value)
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
        let positioned = &self.positioned_layouts;
        let z_index = |root: &NodeId| {
            positioned[nodes[root.index()].layout.index().unwrap()]
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
        self.run_layouts::<true, POSITIONED>(Axis::Horizontal);
        self.measure_wrapped_text(renderer);
        self.run_layouts::<false, POSITIONED>(Axis::Vertical);
        self.run_layouts::<true, POSITIONED>(Axis::Vertical);
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
        self.run_layouts::<false, POSITIONED>(Axis::Horizontal);
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

    fn stored_layout(&self, node: NodeId) -> Option<StoredLayout> {
        let layout = self.nodes[node.index()].layout;
        let index = layout.index()?;
        Some(if layout.is_positioned() {
            self.positioned_layouts[index].layout
        } else {
            self.layouts[index]
        })
    }

    fn run_layouts<const PLACE: bool, const POSITIONED: bool>(&mut self, axis: Axis) {
        if PLACE {
            let mut start = 0;
            while start < self.layout_nodes.len() {
                let vtable = self.stored_layout(self.layout_nodes[start]).unwrap().vtable;
                let mut end = start + 1;
                while end < self.layout_nodes.len()
                    && std::ptr::eq(
                        self.stored_layout(self.layout_nodes[end]).unwrap().vtable,
                        vtable,
                    )
                {
                    end += 1;
                }
                let nodes = unsafe { self.layout_nodes.as_ptr().add(start) };
                // safety: layout storage and node order are frozen during layout
                unsafe { (vtable.place)(nodes, end - start, self, axis, POSITIONED) };
                start = end;
            }
            return;
        }

        let mut end = self.layout_nodes.len();
        while end > 1 {
            let vtable = self
                .stored_layout(self.layout_nodes[end - 1])
                .unwrap()
                .vtable;
            let mut start = end - 1;
            while start > 1
                && std::ptr::eq(
                    self.stored_layout(self.layout_nodes[start - 1])
                        .unwrap()
                        .vtable,
                    vtable,
                )
            {
                start -= 1;
            }
            let nodes = unsafe { self.layout_nodes.as_ptr().add(start) };
            // safety: layout storage and node order are frozen during layout
            unsafe { (vtable.measure)(nodes, end - start, self, axis, POSITIONED) };
            end = start;
        }
    }

    fn layout_offset(&self, node: NodeId) -> LogicalPoint {
        self.stored_layout(node)
            .map_or(LogicalPoint::default(), |layout| layout.offset)
    }

    fn resolve_absolute_children(&mut self, parent: usize, axis: Axis) {
        let mut child = parent + 1;
        let end = self.nodes[parent].subtree_end as usize;
        while child < end {
            let node = &self.nodes[child];
            let next = node.subtree_end as usize;
            if !node.layout.is_positioned() {
                child = next;
                continue;
            }
            let position = self.positioned_layouts[node.layout.index().unwrap()].absolute;
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

    fn transition_area(&self, node: NodeId, mut parent: NodeId) -> LogicalRect {
        let node = node.index();
        if self.nodes[node].layout.is_positioned()
            && self.positioned_layouts[self.nodes[node].layout.index().unwrap()]
                .absolute
                .target
                == PositionTarget::Screen
        {
            parent = NodeId::ROOT;
        }
        let mut area = self.nodes[node].area;
        let parent_node = &self.nodes[parent.index()];
        let parent_offset = if self.nodes[node].layout.is_positioned() {
            LogicalPoint::default()
        } else {
            self.layout_offset(parent)
        };
        // positions transition within the parent and ignore scrolling
        area.x -= parent_node.area.x + parent_offset.x;
        area.y -= parent_node.area.y + parent_offset.y;
        area
    }

    fn prepare_transitioned_dimensions(&mut self, states: &[TransitionState]) {
        for state in states.iter().filter(|state| state.seen) {
            let node = &mut self.nodes[state.node.index()];
            if state.active.contains(TransitionProperties::WIDTH) {
                node.item.width = Sizing::fixed(state.current.width);
            }
            if state.active.contains(TransitionProperties::HEIGHT) {
                node.item.height = Sizing::fixed(state.current.height);
            }
        }
    }

    fn apply_transitioned_positions(&mut self, states: &mut [TransitionState]) {
        states.sort_unstable_by_key(|state| state.node.index());
        self.position_offsets[0] = LogicalPoint::default();
        let mut states = states.iter().filter(|state| state.seen).peekable();
        for index in 1..self.nodes.len() {
            let layout = self.nodes[index].layout;
            let screen_targeted = layout.is_positioned()
                && self.positioned_layouts[layout.index().unwrap()]
                    .absolute
                    .target
                    == PositionTarget::Screen;
            let inherited = if screen_targeted {
                LogicalPoint::default()
            } else {
                self.position_offsets[self.nodes[index].parent.index()]
            };
            let mut local = LogicalPoint::default();
            if let Some(state) = states.next_if(|state| state.node.index() == index) {
                let parent = if screen_targeted {
                    NodeId::ROOT
                } else {
                    state.parent
                };
                let parent_offset = if layout.is_positioned() {
                    LogicalPoint::default()
                } else {
                    self.layout_offset(parent)
                };
                let parent_area = self.nodes[parent.index()].area;
                let parent_delta = self.position_offsets[parent.index()];
                if state.active.contains(TransitionProperties::X) {
                    let target = self.nodes[index].area.x
                        - (parent_area.x - parent_delta.x + parent_offset.x);
                    local.x = state.current.x - target;
                }
                if state.active.contains(TransitionProperties::Y) {
                    let target = self.nodes[index].area.y
                        - (parent_area.y - parent_delta.y + parent_offset.y);
                    local.y = state.current.y - target;
                }
            }
            let offset = LogicalPoint {
                x: inherited.x + local.x,
                y: inherited.y + local.y,
            };
            self.position_offsets[index] = offset;
            self.nodes[index].area.x += offset.x;
            self.nodes[index].area.y += offset.y;
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
            if index != root && node.layout.is_positioned() {
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

unsafe fn run_layout_batch<L: Layout, const PLACE: bool>(
    nodes: *const NodeId,
    len: usize,
    frame: &mut FrameGraph,
    axis: Axis,
    positioned: bool,
) {
    for index in 0..len {
        let index = if PLACE { index } else { len - index - 1 };
        // safety: the scheduler passes a frozen slice of layout node IDs
        let node = unsafe { nodes.add(index).read() };
        let stored = frame.stored_layout(node).unwrap();
        debug_assert!(std::ptr::eq(stored.vtable, &LayoutVtableFor::<L>::VALUE));
        debug_assert!(stored.data_offset as usize + size_of::<L>() <= frame.layout_data.len());
        // safety: store_layout wrote an aligned L at this address
        let layout = unsafe {
            &*frame
                .layout_data
                .as_ptr()
                .add(stored.data_offset as usize)
                .cast::<L>()
        };
        let graph_nodes = frame.nodes.as_ptr();
        let mut cx = LayoutCx {
            frame,
            nodes: graph_nodes,
            node,
            positioned,
            item: std::marker::PhantomData,
            offset: stored.offset,
        };
        if PLACE {
            layout.place(&mut cx, axis);
            if positioned {
                cx.frame.resolve_absolute_children(node.index(), axis);
            }
        } else if let Some(size) = layout.measure(&cx, axis) {
            cx.frame.nodes[node.index()].set_size(axis, size);
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

struct Node {
    parent: NodeId,
    subtree_end: u32,
    item: Item,
    layout: LayoutId,
    layout_item: LayoutItemId,
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

struct DataArena {
    data: NonNull<u8>,
    len: usize,
    capacity: usize,
    align: usize,
    drops: Vec<DropRecord>,
}

impl DataArena {
    fn store<T>(&mut self, value: T) -> u32 {
        let align = align_of::<T>();
        let offset = self
            .len
            .checked_add(align - 1)
            .expect("too much layout data in one frame")
            & !(align - 1);
        let end = offset
            .checked_add(size_of::<T>())
            .expect("too much layout data in one frame");
        if end > self.capacity || align > self.align {
            let capacity = end
                .max(self.capacity.saturating_mul(2))
                .max(64)
                .checked_next_power_of_two()
                .expect("too much layout data in one frame");
            let allocation_align = self.align.max(align);
            let allocation = AllocationLayout::from_size_align(capacity, allocation_align).unwrap();
            let data = NonNull::new(unsafe { alloc(allocation) })
                .unwrap_or_else(|| handle_alloc_error(allocation));
            if self.len != 0 {
                unsafe {
                    std::ptr::copy_nonoverlapping(self.data.as_ptr(), data.as_ptr(), self.len)
                };
            }
            if self.capacity != 0 {
                let previous =
                    AllocationLayout::from_size_align(self.capacity, self.align).unwrap();
                unsafe { dealloc(self.data.as_ptr(), previous) };
            }
            self.data = data;
            self.capacity = capacity;
            self.align = allocation_align;
        }
        unsafe { self.data.as_ptr().add(offset).cast::<T>().write(value) };
        self.len = end;
        if needs_drop::<T>() {
            self.drops.push(DropRecord {
                offset,
                drop: drop_data::<T>,
            });
        }
        u32::try_from(offset).expect("too much layout data in one frame")
    }

    fn clear(&mut self) {
        while let Some(record) = self.drops.pop() {
            unsafe { (record.drop)(self.data.as_ptr().add(record.offset)) };
        }
        self.len = 0;
    }

    fn as_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }

    fn len(&self) -> usize {
        self.len
    }

    fn heap_bytes(&self) -> usize {
        self.capacity + self.drops.capacity() * size_of::<DropRecord>()
    }
}

impl Default for DataArena {
    fn default() -> Self {
        Self {
            data: NonNull::dangling(),
            len: 0,
            capacity: 0,
            align: 1,
            drops: Vec::new(),
        }
    }
}

impl Drop for DataArena {
    fn drop(&mut self) {
        self.clear();
        if self.capacity != 0 {
            let allocation = AllocationLayout::from_size_align(self.capacity, self.align).unwrap();
            unsafe { dealloc(self.data.as_ptr(), allocation) };
        }
    }
}

struct DropRecord {
    offset: usize,
    drop: unsafe fn(*mut u8),
}

unsafe fn drop_data<T>(data: *mut u8) {
    unsafe { data.cast::<T>().drop_in_place() }
}

#[derive(Clone, Copy)]
struct StoredLayout {
    data_offset: u32,
    vtable: &'static LayoutVtable,
    offset: LogicalPoint,
}

type RunLayouts = unsafe fn(*const NodeId, usize, &mut FrameGraph, Axis, positioned: bool);

struct LayoutVtable {
    measure: RunLayouts,
    place: RunLayouts,
    layout_type: fn() -> TypeId,
}

struct LayoutVtableFor<L>(std::marker::PhantomData<L>);

impl<L: Layout> LayoutVtableFor<L> {
    const VALUE: LayoutVtable = LayoutVtable {
        measure: run_layout_batch::<L, false>,
        place: run_layout_batch::<L, true>,
        layout_type: || TypeId::of::<L>(),
    };
}

#[derive(Clone, Copy)]
struct PositionedLayout {
    layout: StoredLayout,
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

pub struct TransitionState {
    pub id: WidgetId,
    pub current: LogicalRect,
    pub initial: LogicalRect,
    pub target: LogicalRect,
    pub started_at: Option<Duration>,
    pub active: TransitionProperties,
    pub node: NodeId,
    pub parent: NodeId,
    pub config: Transition,
    pub initialized: bool,
    pub seen: bool,
}

impl TransitionState {
    pub fn new(id: WidgetId) -> Self {
        Self {
            id,
            current: LogicalRect::default(),
            initial: LogicalRect::default(),
            target: LogicalRect::default(),
            started_at: None,
            active: TransitionProperties::NONE,
            node: NodeId::ROOT,
            parent: NodeId::ROOT,
            config: Transition::new(Duration::ZERO),
            initialized: false,
            seen: false,
        }
    }

    pub fn begin(&mut self, node: NodeId, parent: NodeId, config: Transition) {
        assert!(!self.seen, "duplicate transition WidgetId {:?}", self.id);
        self.node = node;
        self.parent = parent;
        self.config = config;
        self.seen = true;
    }

    pub fn is_active(&self) -> bool {
        self.started_at.is_some()
    }

    pub fn advance(&mut self, target: LogicalRect, now: Duration) {
        if !self.initialized {
            self.current = target;
            self.initial = target;
            self.target = target;
            self.initialized = true;
            return;
        }
        self.active = self.active.intersection(self.config.properties);
        if self.active.is_empty() {
            self.started_at = None;
        }
        if self.started_at.is_some() && self.config.duration.is_zero() {
            self.current = self.target;
            self.started_at = None;
            self.active = TransitionProperties::NONE;
        }
        if let Some(started_at) = self.started_at {
            let progress = (now.saturating_sub(started_at).as_secs_f32()
                / self.config.duration.as_secs_f32())
            .min(1.0);
            let amount = self.config.easing.apply(progress);
            if self.active.contains(TransitionProperties::X) {
                self.current.x = self.initial.x + (self.target.x - self.initial.x) * amount;
            }
            if self.active.contains(TransitionProperties::Y) {
                self.current.y = self.initial.y + (self.target.y - self.initial.y) * amount;
            }
            if self.active.contains(TransitionProperties::WIDTH) {
                self.current.width =
                    self.initial.width + (self.target.width - self.initial.width) * amount;
            }
            if self.active.contains(TransitionProperties::HEIGHT) {
                self.current.height =
                    self.initial.height + (self.target.height - self.initial.height) * amount;
            }
            if progress == 1.0 {
                self.current = self.target;
                self.started_at = None;
                self.active = TransitionProperties::NONE;
            }
        }

        let mut changed = TransitionProperties::NONE;
        if self.config.properties.contains(TransitionProperties::X) && self.target.x != target.x {
            changed = changed.union(TransitionProperties::X);
        }
        if self.config.properties.contains(TransitionProperties::Y) && self.target.y != target.y {
            changed = changed.union(TransitionProperties::Y);
        }
        if self.config.properties.contains(TransitionProperties::WIDTH)
            && self.target.width != target.width
        {
            changed = changed.union(TransitionProperties::WIDTH);
        }
        if self
            .config
            .properties
            .contains(TransitionProperties::HEIGHT)
            && self.target.height != target.height
        {
            changed = changed.union(TransitionProperties::HEIGHT);
        }

        self.target = target;
        if !changed.is_empty() {
            self.initial = self.current;
            self.active = self.active.union(changed);
            if self.config.duration.is_zero() {
                self.current = target;
                self.started_at = None;
                self.active = TransitionProperties::NONE;
            } else {
                self.started_at = Some(now);
            }
        } else if self.started_at.is_none() {
            self.initial = target;
            self.current = target;
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

#[derive(Clone, Copy)]
struct LayoutItemId(u32);

impl LayoutItemId {
    const NONE: Self = Self(0);

    fn new(offset: u32) -> Self {
        Self(
            offset
                .checked_add(1)
                .expect("too much layout data in one frame"),
        )
    }

    fn offset(self) -> Option<usize> {
        self.0.checked_sub(1).map(|offset| offset as usize)
    }
}

/// index into `layouts`, or into `positioned_layouts` when the high bit is set
#[derive(Clone, Copy, Default)]
struct LayoutId(u32);

impl LayoutId {
    const POSITIONED: u32 = 1 << 31;
    const NONE: Self = Self(0);

    fn normal(index: usize) -> Self {
        let id = u32::try_from(index + 1).expect("too many layouts in one frame");
        assert!(id < Self::POSITIONED, "too many layouts in one frame");
        Self(id)
    }

    fn positioned(index: usize) -> Self {
        let id = u32::try_from(index + 1).expect("too many positioned layouts in one frame");
        assert!(
            id < Self::POSITIONED,
            "too many positioned layouts in one frame"
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
