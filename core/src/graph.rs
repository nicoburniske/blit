//! frame-local graph construction, layout resolution, and command emission

use std::{
    any::TypeId,
    mem::{MaybeUninit, align_of, size_of},
    num::NonZeroU16,
    time::Duration,
};

use crate::{
    FrameGraphMemory,
    animation::{Transition, TransitionProperties},
    color::Color,
    command_list::{BoxShadow, ClipId, CommandList, Rectangle},
    container::{Absolute, Anchor, ContainerConfig, LayerId, PositionTarget, Sizing, Slot},
    geometry::{LogicalPoint, LogicalRect, LogicalSize, Sides},
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
    needs_paint_order: bool,
    nodes: Vec<Node>,
    open_containers: Vec<NodeId>,
    texts: Vec<TextContent>,
    images: Vec<ImageContent>,
    layouts: Vec<StoredLayout>,
    positioned_layouts: Vec<PositionedLayout>,
    layout_nodes: Vec<NodeId>,
    layout_data: DataArena,
    position_offsets: Vec<LogicalPoint>,
    layers: Vec<PaintLayer>,
    // empty when declaration order is paint order
    paint_order: Vec<NodeId>,
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
        debug_assert_eq!(self.frame.nodes[node.index()].parent, self.node);
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
        self.frame.layout_data.load(offset)
    }

    /// the sizing requested by a child on an axis
    #[inline]
    pub fn sizing(&self, node: NodeId, axis: Axis) -> crate::container::Sizing {
        debug_assert_eq!(self.frame.nodes[node.index()].parent, self.node);
        self.frame.nodes[node.index()].sizing(axis)
    }

    /// the current size of a child on an axis
    #[inline]
    pub fn axis_size(&self, node: NodeId, axis: Axis) -> f32 {
        debug_assert_eq!(self.frame.nodes[node.index()].parent, self.node);
        self.frame.nodes[node.index()].size(axis)
    }

    /// sets a child's size on an axis without changing its position
    #[inline]
    pub fn set_size(&mut self, node: NodeId, axis: Axis, size: f32) {
        debug_assert_eq!(self.frame.nodes[node.index()].parent, self.node);
        self.frame.nodes[node.index()].set_size(axis, size)
    }

    /// sets a direct child's order among its paint siblings
    #[inline]
    pub fn set_z_index(&mut self, node: NodeId, z_index: i16) {
        assert_eq!(
            self.frame.nodes[node.index()].parent,
            self.node,
            "z-index can only be set for direct children"
        );
        self.frame.nodes[node.index()].slot.z_index = z_index;
        self.frame.needs_paint_order |= z_index != 0;
    }

    /// sets a child's position and size on an axis
    #[inline]
    pub fn set_axis(&mut self, node: NodeId, axis: Axis, position: f32, size: f32) {
        debug_assert_eq!(self.frame.nodes[node.index()].parent, self.node);
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
        self.needs_paint_order = false;
        self.nodes.clear();
        self.open_containers.clear();
        self.texts.clear();
        self.images.clear();
        self.layouts.clear();
        self.positioned_layouts.clear();
        self.layout_nodes.clear();
        self.layout_data.clear();
        self.layers.clear();
        self.paint_order.clear();
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
            slot: Slot {
                layer: None,
                width: Sizing::fixed(screen.width),
                height: Sizing::fixed(screen.height),
                z_index: 0,
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
                .intersects(TransitionProperties::POSITION);
            state.advance(self.transition_area(state.node), time);
            active_transitions = active_transitions.union(state.active);
        }
        if has_position_transition {
            self.position_offsets
                .resize(self.nodes.len(), LogicalPoint::default());
        }
        if active_transitions.intersects(TransitionProperties::SIZE) {
            self.prepare_transitioned_dimensions(transition_states);
            if positioned {
                self.layout::<true>(renderer);
            } else {
                self.layout::<false>(renderer);
            }
        }
        if active_transitions.intersects(TransitionProperties::POSITION) {
            self.apply_transitioned_positions(transition_states);
        }
        self.resolve_paint_order();
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
            container.slot,
            Some(layout),
            container.absolute,
            container.style,
            ContentId::NONE,
            container.clip,
            container.id,
            container.hit,
        );
        self.open_containers.push(node);
        self.layout_nodes.push(node);
        node
    }

    pub fn add_leaf(&mut self, slot: Slot, content: Content<'_>) -> NodeId {
        let style = match content {
            Content::Rectangle(style) => style,
            _ => Style::new(),
        };
        let content = self.store_content(content);
        self.append(
            slot,
            None,
            None,
            style,
            content,
            Clip::None,
            None,
            Sides::all(0.0),
        )
    }

    pub fn set_id(&mut self, node: NodeId, id: WidgetId) {
        self.geometry.push(GeometryRecord {
            node,
            id,
            hit: Sides::all(0.0),
        });
    }

    pub fn add_layer(&mut self) -> LayerId {
        let owner = *self
            .open_containers
            .last()
            .expect("layer declaration requires a root");
        let raw = u16::try_from(self.layers.len() + 1).expect("too many layers in one frame");
        let id = LayerId(NonZeroU16::new(raw).unwrap());
        self.layers.push(PaintLayer {
            owner,
            clip: ClipId::default(),
            clip_bounds: LogicalRect::default(),
        });
        id
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
        self.nodes[node.index()].subtree_end = end;
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
                + self.layers.capacity() * size_of::<PaintLayer>()
                + self.paint_order.capacity() * size_of::<NodeId>()
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
        slot: Slot,
        layout: Option<StoredLayout>,
        position: Option<Absolute>,
        style: Style<'_>,
        content: ContentId,
        clip: Clip,
        id: Option<WidgetId>,
        hit: Sides,
    ) -> NodeId {
        let parent = *self
            .open_containers
            .last()
            .expect("node declaration requires a root");
        let node = NodeId::new(self.nodes.len());
        self.needs_paint_order |= slot.z_index != 0 || slot.layer.is_some();
        let style = self.store_style(style);
        let layout = match (layout, position) {
            (Some(layout), Some(absolute)) => {
                let (target, uses_target_content_origin) = match absolute.target {
                    PositionTarget::Parent => (parent, true),
                    PositionTarget::Widget(id) => {
                        let mut targets = self
                            .geometry
                            .iter()
                            .filter(|record| record.id == id)
                            .map(|record| record.node);
                        let target = targets.next().unwrap_or_else(|| {
                            panic!("absolute target {id:?} must be declared first")
                        });
                        assert!(
                            targets.next().is_none(),
                            "absolute target {id:?} is duplicate"
                        );
                        (target, false)
                    }
                    PositionTarget::Screen => (NodeId::ROOT, false),
                };
                let id = LayoutId::positioned(self.positioned_layouts.len());
                self.positioned_layouts.push(PositionedLayout {
                    layout,
                    offset: absolute.offset,
                    target,
                    target_anchor: absolute.target_anchor,
                    uses_target_content_origin,
                    child_anchor: absolute.child_anchor,
                });
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
            slot,
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
            self.geometry.push(GeometryRecord { node, id, hit });
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

    fn store_data<T: Copy>(&mut self, value: T) -> u32 {
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

    fn resolve_paint_order(&mut self) {
        if !self.needs_paint_order {
            return;
        }

        self.layout_nodes.clear();
        self.layout_nodes.push(NodeId::ROOT);
        while let Some(parent) = self.layout_nodes.pop() {
            if parent != NodeId::ROOT {
                self.paint_order.push(parent);
            }
            let start = self.layout_nodes.len();
            let mut child = parent.index() + 1;
            let end = self.nodes[parent.index()].subtree_end as usize;
            while child < end {
                if self.nodes[child].slot.layer.is_none() {
                    self.layout_nodes.push(NodeId::new(child));
                }
                child = self.nodes[child].subtree_end as usize;
            }
            if parent == NodeId::ROOT && !self.layers.is_empty() {
                for index in 1..self.nodes.len() {
                    if self.nodes[index].slot.layer.is_some() {
                        self.layout_nodes.push(NodeId::new(index));
                    }
                }
            }
            if self.layout_nodes.len() - start <= 1 {
                continue;
            }
            let children = &mut self.layout_nodes[start..];
            // store children in reverse paint order because traversal pops from the end
            if children.iter().any(|node| {
                let slot = self.nodes[node.index()].slot;
                slot.layer.is_some() || slot.z_index != 0
            }) {
                children.sort_unstable_by(|a, b| {
                    let a_index = a.index();
                    let b_index = b.index();
                    (
                        self.nodes[b_index]
                            .slot
                            .layer
                            .map_or(0, |layer| layer.0.get()),
                        self.nodes[b_index].slot.z_index,
                        b_index,
                    )
                        .cmp(&(
                            self.nodes[a_index]
                                .slot
                                .layer
                                .map_or(0, |layer| layer.0.get()),
                            self.nodes[a_index].slot.z_index,
                            a_index,
                        ))
                });
            } else {
                children.reverse();
            }
        }
        debug_assert_eq!(self.paint_order.len(), self.nodes.len() - 1);

        if !self.geometry.is_empty() {
            self.layout_nodes.resize(self.nodes.len(), NodeId::ROOT);
            for (rank, node) in self.paint_order.iter().copied().enumerate() {
                self.layout_nodes[node.index()] = NodeId::new(rank);
            }
            let ranks = &self.layout_nodes;
            self.geometry
                .sort_unstable_by_key(|record| ranks[record.node.index()].index());
        }
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

    fn position_reference(&self, node: NodeId) -> (NodeId, LogicalPoint) {
        let layout = self.nodes[node.index()].layout;
        if layout.is_positioned() {
            let positioned = self.positioned_layouts[layout.index().unwrap()];
            let offset = if positioned.uses_target_content_origin {
                self.layout_offset(positioned.target)
            } else {
                LogicalPoint::default()
            };
            return (positioned.target, offset);
        }
        let parent = self.nodes[node.index()].parent;
        (parent, self.layout_offset(parent))
    }

    fn resolve_positioned(&mut self, node: NodeId, axis: Axis) {
        let positioned = self.positioned_layouts[self.nodes[node.index()].layout.index().unwrap()];
        let (target, target_offset) = self.position_reference(node);
        let target = self.nodes[target.index()].area;
        let (origin, available, offset) = match axis {
            Axis::Horizontal => (
                target.x + target_offset.x,
                target.width,
                positioned.offset.x,
            ),
            Axis::Vertical => (
                target.y + target_offset.y,
                target.height,
                positioned.offset.y,
            ),
        };
        let node = &mut self.nodes[node.index()];
        let size = node.sizing(axis).resolve(node.size(axis), available, true);
        let position = origin + available * positioned.target_anchor.factor(axis)
            - size * positioned.child_anchor.factor(axis)
            + offset;
        node.set_axis(axis, position, size);
    }

    fn transition_area(&self, node: NodeId) -> LogicalRect {
        let mut area = self.nodes[node.index()].area;
        let (parent, parent_offset) = self.position_reference(node);
        let parent_node = &self.nodes[parent.index()];
        // positions transition within the parent and ignore scrolling
        area.x -= parent_node.area.x + parent_offset.x;
        area.y -= parent_node.area.y + parent_offset.y;
        area
    }

    fn prepare_transitioned_dimensions(&mut self, states: &[TransitionState]) {
        for state in states.iter().filter(|state| state.seen) {
            let node = &mut self.nodes[state.node.index()];
            if state.active.intersects(TransitionProperties::WIDTH) {
                node.slot.width = Sizing::fixed(state.current.width);
            }
            if state.active.intersects(TransitionProperties::HEIGHT) {
                node.slot.height = Sizing::fixed(state.current.height);
            }
        }
    }

    fn apply_transitioned_positions(&mut self, states: &mut [TransitionState]) {
        states.sort_unstable_by_key(|state| state.node.index());
        self.position_offsets[0] = LogicalPoint::default();
        let mut states = states.iter().filter(|state| state.seen).peekable();
        for index in 1..self.nodes.len() {
            let (parent, parent_offset) = self.position_reference(NodeId::new(index));
            let inherited = self.position_offsets[parent.index()];
            let mut local = LogicalPoint::default();
            if let Some(state) = states.next_if(|state| state.node.index() == index) {
                let parent_area = self.nodes[parent.index()].area;
                let parent_delta = self.position_offsets[parent.index()];
                if state.active.intersects(TransitionProperties::X) {
                    let target = self.nodes[index].area.x
                        - (parent_area.x - parent_delta.x + parent_offset.x);
                    local.x = state.current.x - target;
                }
                if state.active.intersects(TransitionProperties::Y) {
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
            // todo: index layers by owner. current resolution is O(nodes * layers)
            for layer in self
                .layers
                .iter_mut()
                .filter(|layer| layer.owner.index() == index)
            {
                layer.clip = child_clip;
                layer.clip_bounds = child_clip_bounds;
            }
            let mut child = index + 1;
            let end = self.nodes[index].subtree_end as usize;
            while child < end {
                if let Some(layer) = self.nodes[child].slot.layer {
                    let layer = self
                        .layers
                        .get(layer.0.get() as usize - 1)
                        .expect("layer must be declared in the current frame");
                    self.nodes[child].clip = layer.clip;
                    self.nodes[child].clip_bounds = layer.clip_bounds;
                } else {
                    self.nodes[child].clip = child_clip;
                    self.nodes[child].clip_bounds = child_clip_bounds;
                }
                child = self.nodes[child].subtree_end as usize;
            }
        }
    }

    fn emit(&self, renderer: &mut dyn Renderer, commands: &mut CommandList, scale_factor: f32) {
        if self.paint_order.is_empty() {
            for index in 1..self.nodes.len() {
                self.emit_node(index, renderer, commands, scale_factor);
            }
        } else {
            for node in &self.paint_order {
                self.emit_node(node.index(), renderer, commands, scale_factor);
            }
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
            let area = LogicalRect {
                x: node.area.x - record.hit.left,
                y: node.area.y - record.hit.top,
                width: node.area.width + record.hit.left + record.hit.right,
                height: node.area.height + record.hit.top + record.hit.bottom,
            }
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
        let layout: L = frame.layout_data.load(stored.data_offset as usize);
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
            if positioned && cx.frame.nodes[node.index()].layout.is_positioned() {
                cx.frame.resolve_positioned(node, axis);
            }
            layout.place(&mut cx, axis);
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
    slot: Slot,
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
            Axis::Horizontal => self.slot.width,
            Axis::Vertical => self.slot.height,
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

#[derive(Default)]
struct DataArena {
    words: Vec<Word>,
    len: usize,
}

impl DataArena {
    fn store<T: Copy>(&mut self, value: T) -> u32 {
        const {
            assert!(
                align_of::<T>() <= align_of::<Word>(),
                "layout data alignment exceeds 8 bytes"
            );
        }
        let offset = self
            .len
            .checked_next_multiple_of(align_of::<T>())
            .expect("too much layout data in one frame");
        let end = offset
            .checked_add(size_of::<T>())
            .expect("too much layout data in one frame");
        self.words.resize_with(end.div_ceil(size_of::<Word>()), || {
            Word(MaybeUninit::uninit())
        });
        unsafe {
            self.words
                .as_mut_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<T>()
                .write(value)
        };
        self.len = end;
        u32::try_from(offset).expect("too much layout data in one frame")
    }

    fn clear(&mut self) {
        self.len = 0;
    }

    fn load<T: Copy>(&self, offset: usize) -> T {
        debug_assert!(align_of::<T>() <= align_of::<Word>());
        debug_assert_eq!(offset % align_of::<T>(), 0);
        debug_assert!(
            offset
                .checked_add(size_of::<T>())
                .is_some_and(|end| end <= self.len)
        );
        // safety: store wrote an aligned T at this offset
        unsafe {
            self.words
                .as_ptr()
                .cast::<u8>()
                .add(offset)
                .cast::<T>()
                .read()
        }
    }

    fn heap_bytes(&self) -> usize {
        self.words.capacity() * size_of::<Word>()
    }
}

#[repr(C, align(8))]
struct Word(MaybeUninit<[u8; 8]>);

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
    offset: LogicalPoint,
    target: NodeId,
    target_anchor: Anchor,
    child_anchor: Anchor,
    uses_target_content_origin: bool,
}

struct PaintLayer {
    owner: NodeId,
    clip: ClipId,
    clip_bounds: LogicalRect,
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
            config: Transition::new(Duration::ZERO),
            initialized: false,
            seen: false,
        }
    }

    pub fn begin(&mut self, node: NodeId, config: Transition) {
        assert!(!self.seen, "duplicate transition WidgetId {:?}", self.id);
        self.node = node;
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
            if self.active.intersects(TransitionProperties::X) {
                self.current.x = self.initial.x + (self.target.x - self.initial.x) * amount;
            }
            if self.active.intersects(TransitionProperties::Y) {
                self.current.y = self.initial.y + (self.target.y - self.initial.y) * amount;
            }
            if self.active.intersects(TransitionProperties::WIDTH) {
                self.current.width =
                    self.initial.width + (self.target.width - self.initial.width) * amount;
            }
            if self.active.intersects(TransitionProperties::HEIGHT) {
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
        if self.config.properties.intersects(TransitionProperties::X) && self.target.x != target.x {
            changed = changed.union(TransitionProperties::X);
        }
        if self.config.properties.intersects(TransitionProperties::Y) && self.target.y != target.y {
            changed = changed.union(TransitionProperties::Y);
        }
        if self
            .config
            .properties
            .intersects(TransitionProperties::WIDTH)
            && self.target.width != target.width
        {
            changed = changed.union(TransitionProperties::WIDTH);
        }
        if self
            .config
            .properties
            .intersects(TransitionProperties::HEIGHT)
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
    hit: Sides,
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
