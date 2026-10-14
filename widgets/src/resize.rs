use blit::{Constraints, Context, Interaction, LayoutCx, Point, Scalar, Sense, Size, Ui, Widget};

#[derive(Debug, Default)]
pub struct ResizeState<T> {
    size: Option<Size<T>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeEdge {
    Right,
    Bottom,
    Corner,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeGrip<T> {
    pub edge: ResizeEdge,
    pub interaction: Interaction<T>,
}

blit::builder! {
    pub struct Resize<'a, T: Scalar> {
        new(state: &'a mut ResizeState<T>),
        content<W = ()>: W = (),
        grip<F: FnMut(ResizeGrip<T>) -> G = fn(ResizeGrip<T>) -> (), G>: F = |_| (),
        initial: Size<T> = Size::ZERO,
        minimum: Size<T> = Size::ZERO,
        maximum: Size<T> = Size::uniform(T::UNBOUNDED),
        grip_size: Size<T> = Size::uniform(T::from_f32(1.0)),
    }
}

impl<C: Context<Scalar = T>, T: Scalar, W, F, G> Widget<C> for Resize<'_, T, W, F>
where
    W: Widget<C>,
    F: FnMut(ResizeGrip<T>) -> G,
    G: Widget<C>,
{
    type Response = ();
    fn build(self, mut ui: Ui<'_, C>) {
        let state = self.state;
        let content = self.content;
        let mut grip = self.grip;
        let id = ui.current_widget_id();
        let right_id = id.child("right grip");
        let bottom_id = id.child("bottom grip");
        let corner_id = id.child("corner grip");
        let geometry = ui.geometry(id);
        let right = ui.interact_widget(right_id, Sense::DRAG);
        let bottom = ui.interact_widget(bottom_id, Sense::DRAG);
        let corner = ui.interact_widget(corner_id, Sense::DRAG);
        let right_delta = right.drag.map_or(Point::ZERO, |drag| drag.delta);
        let bottom_delta = bottom.drag.map_or(Point::ZERO, |drag| drag.delta);
        let corner_delta = corner.drag.map_or(Point::ZERO, |drag| drag.delta);
        let delta = Size::new(right_delta.x + corner_delta.x, bottom_delta.y + corner_delta.y);
        if delta != Size::ZERO {
            let size = state
                .size
                .or_else(|| geometry.map(|area| area.size()))
                .unwrap_or(self.initial);
            state.size = Some(size + delta);
        }
        let size = Constraints {
            min: self.minimum,
            max: self.maximum.max(self.minimum),
        }
        .constrain(state.size.unwrap_or(self.initial));
        if let Some(stored) = &mut state.size {
            *stored = size;
        }
        let mut shell = ui.layout(Layout {
            size,
            grip_size: self.grip_size,
        });
        shell.child().item(Item::Content).build(content);
        for (item, edge, grip_id, interaction) in [
            (Item::Right, ResizeEdge::Right, right_id, right),
            (Item::Bottom, ResizeEdge::Bottom, bottom_id, bottom),
            (Item::Corner, ResizeEdge::Corner, corner_id, corner),
        ] {
            shell
                .child()
                .item(item)
                .widget_id(grip_id)
                .build(grip(ResizeGrip { edge, interaction }));
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
