use blit::{Interaction, Platform, Sense, Size, Ui, Widget, WidgetId};
use blit_layout::resize::Item;

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

pub fn build<P, C, F, G>(
    mut ui: Ui<'_, P>,
    state: &mut State,
    id: WidgetId,
    config: Config,
    content: C,
    mut grip: F,
) where
    P: Platform,
    C: Widget<P>,
    F: FnMut(Grip) -> G,
    G: Widget<P>,
{
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
        .layout(blit_layout::resize::Layout {
            size,
            minimum: config.minimum,
            maximum: config.maximum,
            grip_size: config.grip_size,
        })
        .widget_id(id);
    shell.child(Item::Content).build(content);
    for (item, edge, grip_id, interaction) in [
        (Item::Right, Edge::Right, right_id, right),
        (Item::Bottom, Edge::Bottom, bottom_id, bottom),
        (Item::Corner, Edge::Corner, corner_id, corner),
    ] {
        shell
            .child(item)
            .widget_id(grip_id)
            .build(grip(Grip { edge, interaction }));
    }
}
