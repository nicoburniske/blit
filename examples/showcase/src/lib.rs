use std::{collections::VecDeque, time::Duration};

use blit::{
    Anchor, Axis, Constraints, Cx, Interaction, Layout, LayoutCx, Place, Platform, Point, Rect,
    Sense, Sides, Size, Sizing, Widget, WidgetId,
};
pub use blit_std::layout::{Align, Justify};

#[derive(Debug, Default)]
pub struct FpsCounter {
    frame_at: Option<Duration>,
    updated_at: Option<Duration>,
    frames: VecDeque<Duration>,
}

impl FpsCounter {
    pub fn update(&mut self, now: Duration) -> Option<f32> {
        if self.frame_at.replace(now) == Some(now) {
            return None;
        }
        self.frames.push_back(now);
        while self
            .frames
            .front()
            .is_some_and(|frame| now.saturating_sub(*frame) > Duration::from_secs(1))
        {
            self.frames.pop_front();
        }
        if self
            .updated_at
            .is_none_or(|updated| now.saturating_sub(updated) >= Duration::from_millis(250))
        {
            self.updated_at = Some(now);
            if let (Some(first), Some(last)) = (self.frames.front(), self.frames.back())
                && self.frames.len() > 1
            {
                let elapsed = last.saturating_sub(*first).as_secs_f32();
                return Some(self.frames.len().saturating_sub(1) as f32 / elapsed);
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CanvasLayout {
    #[default]
    Flex,
    Wrap,
    Grid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ItemSizing {
    #[default]
    Fixed,
    Fit,
    Grow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasConfig {
    pub layout: CanvasLayout,
    pub axis: Axis,
    pub justify: Justify,
    pub align: Align,
    pub sizing: ItemSizing,
    pub zoom: f32,
    pub gap_steps: u8,
    pub padding_steps: u8,
    pub transitions: bool,
}

impl Default for CanvasConfig {
    fn default() -> Self {
        Self {
            layout: CanvasLayout::Flex,
            axis: Axis::Horizontal,
            justify: Justify::Start,
            align: Align::Center,
            sizing: ItemSizing::Fixed,
            zoom: 1.0,
            gap_steps: 1,
            padding_steps: 1,
            transitions: true,
        }
    }
}

impl CanvasConfig {
    pub fn padding(self, unit: Size) -> Sides {
        let steps = f32::from(self.padding_steps) * self.zoom;
        Sides {
            top: steps * unit.height,
            right: steps * unit.width,
            bottom: steps * unit.height,
            left: steps * unit.width,
        }
    }

    pub fn gap(self, axis: Axis, unit: Size) -> f32 {
        let unit = match axis {
            Axis::Horizontal => unit.width,
            Axis::Vertical => unit.height,
        };
        f32::from(self.gap_steps) * self.zoom * unit
    }

    pub fn item_place(self, index: usize, unit: Size) -> Place {
        let main_steps = 3.0 + (index % 5) as f32;
        let cross_steps = 3.0 + (index % 4) as f32;
        let (main_unit, cross_unit) = match self.axis {
            Axis::Horizontal => (unit.width, unit.height),
            Axis::Vertical => (unit.height, unit.width),
        };
        let natural_main = main_steps * main_unit * self.zoom;
        let natural_cross = cross_steps * cross_unit * self.zoom;
        let main = match self.sizing {
            ItemSizing::Fixed => Sizing::fixed(natural_main),
            ItemSizing::Fit => Sizing::fit().min(2.0 * main_unit).max(natural_main),
            ItemSizing::Grow => Sizing::grow().min(2.0 * main_unit),
        };
        let cross = if self.align == Align::Stretch {
            Sizing::fit()
        } else {
            Sizing::fixed(natural_cross)
        };
        match self.axis {
            Axis::Horizontal => Place::new().width(main).height(cross),
            Axis::Vertical => Place::new().width(cross).height(main),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemSpec {
    pub label: &'static str,
    pub rows: usize,
    pub columns: usize,
    pub badge: Option<Anchor>,
}

pub const ITEMS: [ItemSpec; 10] = [
    ItemSpec {
        label: "1",
        rows: 2,
        columns: 2,
        badge: Some(Anchor::TopRight),
    },
    ItemSpec {
        label: "2",
        rows: 1,
        columns: 1,
        badge: None,
    },
    ItemSpec {
        label: "3",
        rows: 1,
        columns: 2,
        badge: None,
    },
    ItemSpec {
        label: "4",
        rows: 1,
        columns: 1,
        badge: None,
    },
    ItemSpec {
        label: "5",
        rows: 1,
        columns: 2,
        badge: Some(Anchor::BottomLeft),
    },
    ItemSpec {
        label: "6",
        rows: 2,
        columns: 1,
        badge: None,
    },
    ItemSpec {
        label: "7",
        rows: 1,
        columns: 2,
        badge: None,
    },
    ItemSpec {
        label: "8",
        rows: 2,
        columns: 2,
        badge: None,
    },
    ItemSpec {
        label: "9",
        rows: 1,
        columns: 1,
        badge: None,
    },
    ItemSpec {
        label: "10",
        rows: 1,
        columns: 1,
        badge: Some(Anchor::BottomRight),
    },
];

#[derive(Debug, Default)]
pub struct ResizeState {
    size: Option<Size>,
}

impl ResizeState {
    pub fn reset(&mut self) {
        self.size = None;
    }

    fn resize(
        &mut self,
        current: Option<Rect>,
        initial: Size,
        minimum: Size,
        maximum: Size,
        delta: Point,
    ) {
        if delta != Point::ZERO {
            let mut size = self
                .size
                .or_else(|| current.map(|area| area.size()))
                .unwrap_or(initial);
            size.width += delta.x;
            size.height += delta.y;
            self.size = Some(size);
        }
        if let Some(size) = &mut self.size {
            size.width = size
                .width
                .clamp(minimum.width, maximum.width.max(minimum.width));
            size.height = size
                .height
                .clamp(minimum.height, maximum.height.max(minimum.height));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    Right,
    Bottom,
    Corner,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeGrip {
    pub edge: ResizeEdge,
    pub interaction: Interaction,
}

pub struct Resizable<'a, C, F> {
    state: &'a mut ResizeState,
    id: WidgetId,
    initial: Size,
    minimum: Size,
    maximum: Size,
    grip_size: Size,
    content: C,
    grip: F,
}

impl<'a, C, F> Resizable<'a, C, F> {
    pub fn new(
        state: &'a mut ResizeState,
        id: WidgetId,
        initial: Size,
        content: C,
        grip: F,
    ) -> Self {
        Self {
            state,
            id,
            initial,
            minimum: Size::ZERO,
            maximum: Size::uniform(f32::INFINITY),
            grip_size: Size::uniform(1.0),
            content,
            grip,
        }
    }

    pub fn minimum(mut self, size: Size) -> Self {
        self.minimum = size;
        self
    }

    pub fn maximum(mut self, size: Size) -> Self {
        self.maximum = size;
        self
    }

    pub fn grip_size(mut self, size: Size) -> Self {
        self.grip_size = size;
        self
    }
}

impl<P, C, F, G> Widget<P> for Resizable<'_, C, F>
where
    P: Platform,
    C: Widget<P>,
    F: FnMut(ResizeGrip) -> G,
    G: Widget<P>,
{
    type Response = ();

    fn build(self, mut ui: Cx<'_, P>) {
        let Self {
            state,
            id,
            initial,
            minimum,
            maximum,
            grip_size,
            content,
            mut grip,
        } = self;
        let right_id = id.child("right grip");
        let bottom_id = id.child("bottom grip");
        let corner_id = id.child("corner grip");
        let right = ui.interact(right_id, Sense::DRAG);
        let bottom = ui.interact(bottom_id, Sense::DRAG);
        let corner = ui.interact(corner_id, Sense::DRAG);
        state.resize(
            ui.geometry(id),
            initial,
            minimum,
            maximum,
            Point::new(
                right.drag_delta.x + corner.drag_delta.x,
                bottom.drag_delta.y + corner.drag_delta.y,
            ),
        );
        let size = state.size.unwrap_or(initial);
        let mut shell = ui
            .layout(ResizeLayout {
                size,
                minimum,
                maximum,
                grip_size,
            })
            .widget_id(id);
        shell.child().item(ResizeItem::Content).add(content);
        for (item, edge, grip_id, interaction) in [
            (ResizeItem::Right, ResizeEdge::Right, right_id, right),
            (ResizeItem::Bottom, ResizeEdge::Bottom, bottom_id, bottom),
            (ResizeItem::Corner, ResizeEdge::Corner, corner_id, corner),
        ] {
            let widget = grip(ResizeGrip { edge, interaction });
            shell.child().item(item).widget_id(grip_id).add(widget);
        }
    }
}

#[derive(Clone, Copy)]
struct ResizeLayout {
    size: Size,
    minimum: Size,
    maximum: Size,
    grip_size: Size,
}

#[derive(Clone, Copy)]
enum ResizeItem {
    Content,
    Right,
    Bottom,
    Corner,
}

impl<P: Platform> Layout<P> for ResizeLayout {
    type Item = ResizeItem;

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, constraints: Constraints) -> Size {
        let maximum = self.maximum.max(self.minimum);
        let size = constraints.constrain(Size::new(
            cx.resolve_extent(
                Axis::Horizontal,
                self.size.width.clamp(self.minimum.width, maximum.width),
            ),
            cx.resolve_extent(
                Axis::Vertical,
                self.size.height.clamp(self.minimum.height, maximum.height),
            ),
        ));
        let grip = Size::new(
            cx.resolve_extent(Axis::Horizontal, self.grip_size.width)
                .min(size.width),
            cx.resolve_extent(Axis::Vertical, self.grip_size.height)
                .min(size.height),
        );
        for child in cx.children() {
            let (position, child_size, z_index) = match cx.item(child) {
                ResizeItem::Content => (Point::ZERO, size, 0),
                ResizeItem::Right => (
                    Point::new(size.width - grip.width, 0.0),
                    Size::new(grip.width, size.height),
                    1,
                ),
                ResizeItem::Bottom => (
                    Point::new(0.0, size.height - grip.height),
                    Size::new(size.width, grip.height),
                    1,
                ),
                ResizeItem::Corner => (
                    Point::new(size.width - grip.width, size.height - grip.height),
                    grip,
                    2,
                ),
            };
            cx.layout_child(child, Constraints::tight(child_size));
            cx.set_position(child, position);
            cx.set_z_index(child, z_index);
        }
        size
    }
}
