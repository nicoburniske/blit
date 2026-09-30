use blit::{
    Constraints, Interaction, IntrinsicQuery, IntrinsicSize, Layout as LayoutTrait, LayoutCx, LogicalPoint,
    LogicalSize, MeasureCx, Sense, Ui, Widget,
};
use blit_layout::Unit;

#[derive(Debug, Default)]
pub struct State {
    size: Option<LogicalSize>,
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
    #[const]
    #[derive(Clone, Copy, Debug)]
    pub struct Config<U: Unit> {
        new(initial: LogicalSize),
        minimum: LogicalSize = LogicalSize::ZERO,
        maximum: LogicalSize = LogicalSize::uniform(f32::INFINITY),
        grip_size: blit::Size<U> = blit::Size::uniform(U::ONE),
    }
}

pub fn new<'a, C, U: Unit, W, F, G>(
    state: &'a mut State,
    config: Config<U>,
    content: W,
    mut grip: F,
) -> impl Widget<C> + 'a
where
    W: Widget<C> + 'a,
    F: FnMut(Grip) -> G + 'a,
    G: Widget<C>,
{
    move |mut ui: Ui<'_, C>| {
        let id = ui.current_widget_id();
        let right_id = id.child("right grip");
        let bottom_id = id.child("bottom grip");
        let corner_id = id.child("corner grip");
        let geometry = ui.geometry(id);
        let right = ui.interact_widget(right_id, Sense::DRAG);
        let bottom = ui.interact_widget(bottom_id, Sense::DRAG);
        let corner = ui.interact_widget(corner_id, Sense::DRAG);
        let delta = LogicalSize::new(
            right.drag_delta.x + corner.drag_delta.x,
            bottom.drag_delta.y + corner.drag_delta.y,
        );
        if delta != LogicalSize::ZERO {
            let mut size = state
                .size
                .or_else(|| geometry.map(|area| area.size()))
                .unwrap_or(config.initial);
            size.width += delta.width;
            size.height += delta.height;
            state.size = Some(size);
        }
        if let Some(size) = &mut state.size {
            size.width = size
                .width
                .clamp(config.minimum.width, config.maximum.width.max(config.minimum.width));
            size.height = size
                .height
                .clamp(config.minimum.height, config.maximum.height.max(config.minimum.height));
        }
        let size = state.size.unwrap_or(config.initial);
        let mut shell = ui.layout(Layout {
            size,
            minimum: config.minimum,
            maximum: config.maximum,
            grip_size: config.grip_size,
        });
        shell.child().item(Item::Content).build(content);
        for (item, edge, grip_id, interaction) in [
            (Item::Right, Edge::Right, right_id, right),
            (Item::Bottom, Edge::Bottom, bottom_id, bottom),
            (Item::Corner, Edge::Corner, corner_id, corner),
        ] {
            shell
                .child()
                .item(item)
                .widget_id(grip_id)
                .build(grip(Grip { edge, interaction }));
        }
    }
}

#[derive(Clone, Copy, Default)]
enum Item {
    #[default]
    Content,
    Right,
    Bottom,
    Corner,
}

#[derive(Clone, Copy)]
struct Layout<U: Unit> {
    size: LogicalSize,
    minimum: LogicalSize,
    maximum: LogicalSize,
    grip_size: blit::Size<U>,
}

impl<C, U: Unit> LayoutTrait<C> for Layout<U> {
    type Item = Item;

    fn intrinsic(&self, _: &mut MeasureCx<'_, C, Self::Item>, query: IntrinsicQuery) -> IntrinsicSize {
        let min = U::round(query.axis.extent(self.minimum)).max(0.0);
        let max = query.axis.extent(self.maximum).max(min);
        let preferred = U::round(query.axis.extent(self.size).clamp(min, max));
        IntrinsicSize::new(min, preferred.max(min))
    }

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints) -> LogicalSize {
        let maximum = self.maximum.max(self.minimum);
        let size = constraints.constrain(LogicalSize::new(
            U::round(self.size.width.clamp(self.minimum.width, maximum.width)),
            U::round(self.size.height.clamp(self.minimum.height, maximum.height)),
        ));
        let grip = LogicalSize::new(
            self.grip_size.width.into_float().min(size.width),
            self.grip_size.height.into_float().min(size.height),
        );
        for child in cx.children() {
            let (position, child_size, z_index) = match *child.item {
                Item::Content => (LogicalPoint::ZERO, size, 0),
                Item::Right => (
                    LogicalPoint::new(size.width - grip.width, 0.0),
                    LogicalSize::new(grip.width, size.height),
                    1,
                ),
                Item::Bottom => (
                    LogicalPoint::new(0.0, size.height - grip.height),
                    LogicalSize::new(size.width, grip.height),
                    1,
                ),
                Item::Corner => (
                    LogicalPoint::new(size.width - grip.width, size.height - grip.height),
                    grip,
                    2,
                ),
            };
            cx.layout_child(child.id, Constraints::tight(child_size));
            cx.set_position(child.id, position);
            cx.set_child_z_index(child.id, z_index);
        }
        size
    }
}
