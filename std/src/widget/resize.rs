use blit::{
    Axis, Constraints, Interaction, Layout, LayoutCx, Place, Platform, Point, Sense, Size, Ui,
    Widget, WidgetId,
};

#[derive(Debug, Default)]
pub struct State {
    size: Option<Size>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Right,
    Bottom,
    Corner,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grip {
    pub edge: Edge,
    pub interaction: Interaction,
}

pub struct Area<'a, C, F> {
    state: &'a mut State,
    id: WidgetId,
    initial: Size,
    minimum: Size,
    maximum: Size,
    grip_size: Size,
    content: C,
    grip: F,
}

impl<'a, C, F> Area<'a, C, F> {
    pub fn new(state: &'a mut State, id: WidgetId, initial: Size, content: C, grip: F) -> Self {
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

impl<P, C, F, G> Widget<P> for Area<'_, C, F>
where
    P: Platform,
    C: Widget<P>,
    F: FnMut(Grip) -> G,
    G: Widget<P>,
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, P>) {
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
        let delta = Point::new(
            right.drag_delta.x + corner.drag_delta.x,
            bottom.drag_delta.y + corner.drag_delta.y,
        );
        if delta != Point::ZERO {
            let mut size = state
                .size
                .or_else(|| ui.geometry(id).map(|area| area.size()))
                .unwrap_or(initial);
            size.width += delta.x;
            size.height += delta.y;
            state.size = Some(size);
        }
        if let Some(size) = &mut state.size {
            size.width = size
                .width
                .clamp(minimum.width, maximum.width.max(minimum.width));
            size.height = size
                .height
                .clamp(minimum.height, maximum.height.max(minimum.height));
        }
        let size = state.size.unwrap_or(initial);
        let mut shell = ui
            .layout(ResizeLayout {
                size,
                minimum,
                maximum,
                grip_size,
            })
            .widget_id(id);
        shell.child(Place::item(ResizeItem::Content)).build(content);
        for (item, edge, grip_id, interaction) in [
            (ResizeItem::Right, Edge::Right, right_id, right),
            (ResizeItem::Bottom, Edge::Bottom, bottom_id, bottom),
            (ResizeItem::Corner, Edge::Corner, corner_id, corner),
        ] {
            shell
                .child(Place::item(item))
                .widget_id(grip_id)
                .build(grip(Grip { edge, interaction }));
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

    fn size_override(&self, _: &mut Self::Item, _: Option<f32>, _: Option<f32>) {}

    fn layout(&self, cx: &mut LayoutCx<'_, P, Self::Item>, constraints: Constraints) -> Size {
        let res = cx.layout_resolution();
        let maximum = self.maximum.max(self.minimum);
        let size = constraints.constrain(Size::new(
            res.extent(
                Axis::Horizontal,
                self.size.width.clamp(self.minimum.width, maximum.width),
            ),
            res.extent(
                Axis::Vertical,
                self.size.height.clamp(self.minimum.height, maximum.height),
            ),
        ));
        let grip = Size::new(
            res.extent(Axis::Horizontal, self.grip_size.width)
                .min(size.width),
            res.extent(Axis::Vertical, self.grip_size.height)
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
