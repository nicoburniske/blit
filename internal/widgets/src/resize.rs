use blit::{
    Axis, Constraints, Interaction, Layout as LayoutTrait, LayoutCx, Point, Sense, Size, Ui,
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

blit::builder! {
    #[derive(Clone, Copy, Debug)]
    pub struct Config {
        new(initial: Size),
        minimum: Size = Size::ZERO,
        maximum: Size = Size::uniform(f32::INFINITY),
        grip_size: Size = Size::uniform(1.0),
    }
}

pub fn new<'a, C, W, F, G>(
    state: &'a mut State,
    id: WidgetId,
    config: Config,
    content: W,
    mut grip: F,
) -> impl Widget<C> + 'a
where
    W: Widget<C> + 'a,
    F: FnMut(Grip) -> G + 'a,
    G: Widget<C>,
{
    move |mut ui: Ui<'_, C>| {
        let right_id = id.child("right grip");
        let bottom_id = id.child("bottom grip");
        let corner_id = id.child("corner grip");
        let right = ui.interact(right_id, Sense::DRAG);
        let bottom = ui.interact(bottom_id, Sense::DRAG);
        let corner = ui.interact(corner_id, Sense::DRAG);
        let delta = Size::new(
            right.drag_delta.x + corner.drag_delta.x,
            bottom.drag_delta.y + corner.drag_delta.y,
        );
        if delta != Size::ZERO {
            let mut size = state
                .size
                .or_else(|| ui.geometry(id).map(|area| area.size()))
                .unwrap_or(config.initial);
            size.width += delta.width;
            size.height += delta.height;
            state.size = Some(size);
        }
        if let Some(size) = &mut state.size {
            size.width = size.width.clamp(
                config.minimum.width,
                config.maximum.width.max(config.minimum.width),
            );
            size.height = size.height.clamp(
                config.minimum.height,
                config.maximum.height.max(config.minimum.height),
            );
        }
        let size = state.size.unwrap_or(config.initial);
        let mut shell = ui
            .layout(Layout {
                size,
                minimum: config.minimum,
                maximum: config.maximum,
                grip_size: config.grip_size,
            })
            .widget_id(id);
        shell.child_item(Item::Content).build(content);
        for (item, edge, grip_id, interaction) in [
            (Item::Right, Edge::Right, right_id, right),
            (Item::Bottom, Edge::Bottom, bottom_id, bottom),
            (Item::Corner, Edge::Corner, corner_id, corner),
        ] {
            shell
                .child_item(item)
                .widget_id(grip_id)
                .build(grip(Grip { edge, interaction }));
        }
    }
}

#[derive(Clone, Copy)]
enum Item {
    Content,
    Right,
    Bottom,
    Corner,
}

#[derive(Clone, Copy)]
struct Layout {
    size: Size,
    minimum: Size,
    maximum: Size,
    grip_size: Size,
}

impl<C> LayoutTrait<C> for Layout {
    type Item = Item;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints) -> Size {
        let res = cx.resolution();
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
            let (position, child_size, z_index) = match *cx.item(child) {
                Item::Content => (Point::ZERO, size, 0),
                Item::Right => (
                    Point::new(size.width - grip.width, 0.0),
                    Size::new(grip.width, size.height),
                    1,
                ),
                Item::Bottom => (
                    Point::new(0.0, size.height - grip.height),
                    Size::new(size.width, grip.height),
                    1,
                ),
                Item::Corner => (
                    Point::new(size.width - grip.width, size.height - grip.height),
                    grip,
                    2,
                ),
            };
            cx.layout_child(child, Constraints::tight(child_size));
            cx.set_child_position(child, position);
            cx.set_child_z_index(child, z_index);
        }
        size
    }

    fn override_size(&self, _: &mut Self::Item, _: Option<f32>, _: Option<f32>) -> bool {
        false
    }
}
