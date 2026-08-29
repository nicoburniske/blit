//! frame-local graph construction, layout resolution, and command emission

mod layout;
mod node;
mod transition;

pub use layout::{Children, LayoutCx};
pub use transition::TransitionState;

use layout::{DataArena, LayoutVtableFor, PositionedLayout, StoredLayout};
use node::*;

use std::{any::TypeId, mem::size_of, num::NonZeroU16, time::Duration};

use crate::{
    FrameGraphMemory,
    animation::{Transition, TransitionProperties},
    color::Color,
    command_list::{BoxShadow, ClipId, CommandList, Rectangle},
    container::{Absolute, Anchor, ContainerConfig, LayerId, PositionTarget, Sizing, Slot},
    geometry::{LogicalPoint, LogicalRect, LogicalSize, Sides},
    image::{ImageContent, ImageRequest},
    interact::{InteractionState, WidgetId},
    layout::{Axis, Constraints, Flex, Layout, LayoutResolution},
    node::{Content, NodeId},
    renderer::Renderer,
    style::{Border, BorderRadius, Clip, LinearGradient, Shadow, Style},
    text::{TextContent, TextLayoutRequest, TextRequest, TextWrap},
};

#[derive(Default)]
pub struct FrameGraph {
    clear: bool,
    needs_paint_order: bool,
    layout_resolution: LayoutResolution,
    nodes: Vec<Node>,
    open_containers: Vec<NodeId>,
    texts: Vec<TextContent>,
    images: Vec<ImageContent>,
    layouts: Vec<StoredLayout>,
    positioned_layouts: Vec<PositionedLayout>,
    layout_nodes: Vec<NodeId>,
    layout_data: DataArena,
    layers: Vec<PaintLayer>,
    layer_order: Vec<LayerId>,
    // empty when declaration order is paint order
    paint_order: Vec<NodeId>,
    styles: Vec<StoredStyle>,
    shadows: Vec<Shadow>,
    clip_specs: Vec<Clip>,
    geometry: Vec<GeometryRecord>,
    gradient_stops: Vec<crate::style::GradientStop>,
}

impl FrameGraph {
    pub fn begin(&mut self, screen: LogicalRect, layout_resolution: LayoutResolution) {
        #[cfg(debug_assertions)]
        generation::begin();
        let root = self.node_id(0);
        self.clear = false;
        self.needs_paint_order = false;
        self.layout_resolution = layout_resolution;
        self.nodes.clear();
        self.open_containers.clear();
        self.texts.clear();
        self.images.clear();
        self.layouts.clear();
        self.positioned_layouts.clear();
        self.layout_nodes.clear();
        self.layout_data.clear();
        self.layers.clear();
        self.layer_order.clear();
        self.paint_order.clear();
        self.styles.clear();
        self.shadows.clear();
        self.clip_specs.clear();
        self.geometry.clear();
        self.gradient_stops.clear();
        let layout = self.store_layout(Flex::column(), LogicalPoint::default());
        self.layouts.push(layout);
        self.layout_nodes.push(root);
        let root_layout = LayoutId::normal(0);
        self.nodes.push(Node {
            parent: root,
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
        self.open_containers.push(root);
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
            [self.node_id(0)],
            "a container scope was not dropped"
        );
        self.nodes[0].subtree_end =
            u32::try_from(self.nodes.len()).expect("too many nodes in one frame");
        let positioned = !self.positioned_layouts.is_empty();
        self.layout(renderer, positioned);
        let mut active_transitions = TransitionProperties::NONE;
        for state in transition_states.iter_mut().filter(|state| state.seen) {
            state.advance(self.transition_area(state.node), time);
            active_transitions = active_transitions.union(state.active);
        }
        if active_transitions.intersects(TransitionProperties::SIZE) {
            self.prepare_transitioned_dimensions(transition_states);
            self.layout(renderer, positioned);
        }
        if active_transitions.intersects(TransitionProperties::POSITION) {
            self.apply_transitioned_positions(transition_states);
        }
        self.resolve_positions();
        self.resolve_paint_order();
        if self.clear {
            commands.push_clear(self.nodes[0].area.to_physical(scale_factor));
        }
        self.resolve_clips(commands);
        self.emit(renderer, commands, scale_factor);
        self.register_hits(renderer, interaction);
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
        let id = LayerId(
            NonZeroU16::new(raw).unwrap(),
            #[cfg(debug_assertions)]
            generation::get(),
        );
        self.layers.push(PaintLayer {
            owner,
            clip: ClipId::default(),
            clip_bounds: LogicalRect::default(),
        });
        self.layer_order.push(id);
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
        self.node_id(self.nodes.len())
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
                + self.layers.capacity() * size_of::<PaintLayer>()
                + self.layer_order.capacity() * size_of::<LayerId>()
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
        let slot = Slot {
            width: self.layout_resolution.sizing(Axis::Horizontal, slot.width),
            height: self.layout_resolution.sizing(Axis::Vertical, slot.height),
            ..slot
        };
        let parent = *self
            .open_containers
            .last()
            .expect("node declaration requires a root");
        let node = self.node_id(self.nodes.len());
        self.needs_paint_order |= slot.z_index != 0 || slot.layer.is_some();
        let style = self.store_style(style);
        let layout = match (layout, position) {
            (Some(layout), Some(absolute)) => {
                let (target, uses_target_content_origin) = match absolute.target {
                    PositionTarget::Parent => (parent, true),
                    PositionTarget::Node(target) => {
                        assert!(
                            target.index() < self.nodes.len(),
                            "absolute target must be declared first"
                        );
                        (target, false)
                    }
                    PositionTarget::Screen => (self.node_id(0), false),
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
        let root = self.node_id(0);
        self.layout_nodes.push(root);
        while let Some(parent) = self.layout_nodes.pop() {
            if parent != root {
                self.paint_order.push(parent);
            }
            let start = self.layout_nodes.len();
            let mut child = parent.index() + 1;
            let end = self.nodes[parent.index()].subtree_end as usize;
            while child < end {
                if self.nodes[child].slot.layer.is_none() {
                    self.layout_nodes.push(self.node_id(child));
                }
                child = self.nodes[child].subtree_end as usize;
            }
            if parent == root && !self.layers.is_empty() {
                for index in 1..self.nodes.len() {
                    if self.nodes[index].slot.layer.is_some() {
                        let node = self.node_id(index);
                        self.layout_nodes.push(node);
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
            self.layout_nodes.resize(self.nodes.len(), root);
            for (rank, node) in self.paint_order.iter().copied().enumerate() {
                self.layout_nodes[node.index()] = self.node_id(rank);
            }
            let ranks = &self.layout_nodes;
            self.geometry
                .sort_unstable_by_key(|record| ranks[record.node.index()].index());
        }
    }

    fn layout(&mut self, renderer: &mut dyn Renderer, positioned: bool) {
        let root = self.node_id(0);
        let area = self.nodes[0].area;
        self.layout_node(
            root,
            Constraints::tight(LogicalSize {
                width: area.width,
                height: area.height,
            }),
            renderer,
            positioned,
        );
        if positioned {
            for index in 1..self.layout_nodes.len() {
                let node = self.layout_nodes[index];
                if self.nodes[node.index()].layout.is_positioned() {
                    self.layout_positioned(node, renderer);
                }
            }
        }
    }

    fn layout_node(
        &mut self,
        node: NodeId,
        constraints: Constraints,
        renderer: &mut dyn Renderer,
        positioned: bool,
    ) -> LogicalSize {
        let size = if let Some(layout) = self.stored_layout(node) {
            (layout.vtable.run)(node, layout, self, constraints, renderer, positioned)
        } else {
            let intrinsic = match self.nodes[node.index()].content.decode() {
                ContentRef::None => LogicalSize::default(),
                ContentRef::Text(index) => {
                    let text = self.texts[index];
                    renderer.measure_text(&TextLayoutRequest {
                        text: text.text,
                        style: text.style,
                        wrap: text.options.wrap,
                        max_width: (text.options.wrap != TextWrap::None
                            && constraints.max.width.is_finite())
                        .then_some(constraints.max.width),
                        max_lines: text.options.max_lines,
                    })
                }
                ContentRef::Image(index) => self.images[index].intrinsic,
            };
            constraints.constrain(intrinsic)
        };
        self.nodes[node.index()].area.width = size.width;
        self.nodes[node.index()].area.height = size.height;
        size
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

    fn layout_offset(&self, node: NodeId) -> LogicalPoint {
        self.stored_layout(node)
            .map_or(LogicalPoint::default(), |layout| layout.offset)
    }

    fn position_offset(&self, node: NodeId) -> LogicalPoint {
        let layout = self.nodes[node.index()].layout;
        if layout.is_positioned() {
            let positioned = self.positioned_layouts[layout.index().unwrap()];
            return if positioned.uses_target_content_origin {
                self.layout_offset(positioned.target)
            } else {
                LogicalPoint::default()
            };
        }
        self.layout_offset(self.nodes[node.index()].parent)
    }

    fn layout_positioned(&mut self, node: NodeId, renderer: &mut dyn Renderer) {
        fn anchor_factor(anchor: Anchor) -> LogicalPoint {
            match anchor {
                Anchor::TopLeft => LogicalPoint { x: 0.0, y: 0.0 },
                Anchor::Top => LogicalPoint { x: 0.5, y: 0.0 },
                Anchor::TopRight => LogicalPoint { x: 1.0, y: 0.0 },
                Anchor::Left => LogicalPoint { x: 0.0, y: 0.5 },
                Anchor::Center => LogicalPoint { x: 0.5, y: 0.5 },
                Anchor::Right => LogicalPoint { x: 1.0, y: 0.5 },
                Anchor::BottomLeft => LogicalPoint { x: 0.0, y: 1.0 },
                Anchor::Bottom => LogicalPoint { x: 0.5, y: 1.0 },
                Anchor::BottomRight => LogicalPoint { x: 1.0, y: 1.0 },
            }
        }

        let positioned = self.positioned_layouts[self.nodes[node.index()].layout.index().unwrap()];
        let target = self.nodes[positioned.target.index()].area;
        let available = LogicalSize {
            width: target.width,
            height: target.height,
        };
        let range = |sizing: Sizing, available: f32| match sizing {
            Sizing::Fit { min, max } => {
                let min = min.max(0.0);
                (min, max.max(min).min(available).max(min))
            }
            Sizing::Grow { .. } => {
                let size = sizing.clamp(available);
                (size, size)
            }
            Sizing::Fixed(size) => {
                let size = size.max(0.0);
                (size, size)
            }
            Sizing::Percent(_) => {
                let size = sizing.resolve(0.0, available, true);
                (size, size)
            }
        };
        let width = range(self.nodes[node.index()].slot.width, available.width);
        let height = range(self.nodes[node.index()].slot.height, available.height);
        let size = self.layout_node(
            node,
            Constraints {
                min: LogicalSize {
                    width: width.0,
                    height: height.0,
                },
                max: LogicalSize {
                    width: width.1,
                    height: height.1,
                },
            },
            renderer,
            true,
        );
        let target_factor = anchor_factor(positioned.target_anchor);
        let child_factor = anchor_factor(positioned.child_anchor);
        let reference_offset = self.position_offset(node);
        self.nodes[node.index()].area.x = available.width * target_factor.x
            - size.width * child_factor.x
            + positioned.offset.x
            + reference_offset.x;
        self.nodes[node.index()].area.y = available.height * target_factor.y
            - size.height * child_factor.y
            + positioned.offset.y
            + reference_offset.y;
    }

    fn resolve_positions(&mut self) {
        for index in 1..self.nodes.len() {
            let layout = self.nodes[index].layout;
            let reference = if layout.is_positioned() {
                self.positioned_layouts[layout.index().unwrap()].target
            } else {
                self.nodes[index].parent
            };
            self.nodes[index].area.x += self.nodes[reference.index()].area.x;
            self.nodes[index].area.y += self.nodes[reference.index()].area.y;
        }
    }

    fn transition_area(&self, node: NodeId) -> LogicalRect {
        let mut area = self.nodes[node.index()].area;
        let offset = self.position_offset(node);
        area.x -= offset.x;
        area.y -= offset.y;
        area
    }

    fn prepare_transitioned_dimensions(&mut self, states: &[TransitionState]) {
        for state in states.iter().filter(|state| state.seen) {
            let node = &mut self.nodes[state.node.index()];
            if state.active.intersects(TransitionProperties::WIDTH) {
                node.slot.width = self
                    .layout_resolution
                    .sizing(Axis::Horizontal, Sizing::fixed(state.current.width));
            }
            if state.active.intersects(TransitionProperties::HEIGHT) {
                node.slot.height = self
                    .layout_resolution
                    .sizing(Axis::Vertical, Sizing::fixed(state.current.height));
            }
        }
    }

    fn apply_transitioned_positions(&mut self, states: &[TransitionState]) {
        for state in states.iter().filter(|state| state.seen) {
            let offset = self.position_offset(state.node);
            let area = &mut self.nodes[state.node.index()].area;
            if state.active.intersects(TransitionProperties::X) {
                area.x = state.current.x + offset.x;
            }
            if state.active.intersects(TransitionProperties::Y) {
                area.y = state.current.y + offset.y;
            }
        }
    }

    fn resolve_clips(&mut self, commands: &mut CommandList) {
        self.nodes[0].clip = ClipId::default();
        self.layer_order
            .sort_unstable_by_key(|layer| self.layers[layer.index()].owner.index());
        let mut next_layer = 0;
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
            // resolve this owner's layers until the next owner is reached
            while next_layer < self.layer_order.len() {
                let layer = self.layer_order[next_layer].index();
                if self.layers[layer].owner.index() != index {
                    break;
                }
                let layer = &mut self.layers[layer];
                layer.clip = child_clip;
                layer.clip_bounds = child_clip_bounds;
                next_layer += 1;
            }
            let mut child = index + 1;
            let end = self.nodes[index].subtree_end as usize;
            while child < end {
                if let Some(layer) = self.nodes[child].slot.layer {
                    let layer = self
                        .layers
                        .get(layer.index())
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
                    let width = caret.width.max(cursor.width).min(node.area.width);
                    let x = cursor.x.clamp(
                        node.area.x,
                        (node.area.x + node.area.width - width).max(node.area.x),
                    );
                    let top = cursor.y.max(node.area.y);
                    let bottom = (cursor.y + cursor.height).min(node.area.y + node.area.height);
                    if bottom > top {
                        let area = LogicalRect {
                            x,
                            y: top,
                            width,
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

    fn register_hits(&self, renderer: &dyn Renderer, interaction: &mut InteractionState) {
        interaction.register_hits(self.geometry.iter().map(|record| {
            let node = &self.nodes[record.node.index()];
            let area = LogicalRect {
                x: node.area.x - record.hit.left,
                y: node.area.y - record.hit.top,
                width: node.area.width + record.hit.left + record.hit.right,
                height: node.area.height + record.hit.top + record.hit.bottom,
            };
            let area = renderer.interaction_area(area, node.clip_bounds);
            (record.id, area)
        }));
    }

    fn node_id(&self, index: usize) -> NodeId {
        NodeId {
            value: u32::try_from(index + 1).expect("too many nodes in one frame"),
            #[cfg(debug_assertions)]
            generation: generation::get(),
        }
    }
}

#[cfg(debug_assertions)]
pub mod generation {
    use std::{
        cell::Cell,
        sync::atomic::{AtomicU16, Ordering},
    };

    static NEXT: AtomicU16 = AtomicU16::new(1);

    thread_local! {
        static CURRENT: Cell<u16> = const { Cell::new(0) };
    }

    pub fn begin() {
        CURRENT.set(NEXT.fetch_add(1, Ordering::Relaxed));
    }

    pub fn get() -> u16 {
        CURRENT.get()
    }

    #[inline]
    pub fn assert(id: u16) {
        assert_eq!(id, get(), "id belongs to another frame");
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
