use blit::{Constraints, Context, Interaction, LayoutCx, Point, Scalar, Sense, Size, Ui, Widget};

#[derive(Debug, Default)]
pub struct State<T> {
    size: Option<Size<T>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Right,
    Bottom,
    Corner,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grip<T> {
    pub edge: Edge,
    pub interaction: Interaction<T>,
}

blit::builder! {
    #[derive(Clone, Copy, Debug)]
    pub struct Config<T: Scalar> {
        new(initial: Size<T>),
        minimum: Size<T> = Size::ZERO,
        maximum: Size<T> = Size::uniform(T::UNBOUNDED),
        grip_size: Size<T> = Size::uniform(T::from_f32(1.0)),
    }
}

pub fn new<'a, C: Context, W, F, G>(
    state: &'a mut State<C::Scalar>,
    config: Config<C::Scalar>,
    content: W,
    mut grip: F,
) -> impl Widget<C> + 'a
where
    W: Widget<C> + 'a,
    F: FnMut(Grip<C::Scalar>) -> G + 'a,
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
        let delta = Size::new(
            right.drag_delta.x + corner.drag_delta.x,
            bottom.drag_delta.y + corner.drag_delta.y,
        );
        if delta != Size::ZERO {
            let size = state
                .size
                .or_else(|| geometry.map(|area| area.size()))
                .unwrap_or(config.initial);
            state.size = Some(size + delta);
        }
        let size = Constraints {
            min: config.minimum,
            max: config.maximum.max(config.minimum),
        }
        .constrain(state.size.unwrap_or(config.initial));
        if let Some(stored) = &mut state.size {
            *stored = size;
        }
        let mut shell = ui.layout(Layout {
            size,
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
struct Layout<T> {
    size: Size<T>,
    grip_size: Size<T>,
}

impl<C: Context<Scalar = T>, T: Scalar> blit::Layout<C> for Layout<T> {
    type Item = Item;

    fn layout(&self, cx: &mut LayoutCx<'_, C, Self::Item>, bounds: Constraints<T>) -> Size<T> {
        let size = bounds.constrain(self.size);
        let grip = Size::new(
            self.grip_size.width.max(T::ZERO).min(size.width),
            self.grip_size.height.max(T::ZERO).min(size.height),
        );
        for child in cx.children() {
            let (position, child_size, z_index) = match *cx.item(child) {
                Item::Content => (Point::ZERO, size, 0),
                Item::Right => (
                    Point::new(size.width - grip.width, T::ZERO),
                    Size::new(grip.width, size.height),
                    1,
                ),
                Item::Bottom => (
                    Point::new(T::ZERO, size.height - grip.height),
                    Size::new(size.width, grip.height),
                    1,
                ),
                Item::Corner => (Point::new(size.width - grip.width, size.height - grip.height), grip, 2),
            };
            cx.layout_child(child, Constraints::tight(child_size));
            cx.set_child_position(child, position);
            cx.set_child_z_index(child, z_index);
        }
        size
    }
}
