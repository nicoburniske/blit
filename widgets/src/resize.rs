use blit::{Constraints, Context, Interaction, LayoutCx, Point, Press, Scalar, Size, Ui, Widget};

use crate::Drag;

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
pub struct ResizeGrip {
    pub edge: ResizeEdge,
    pub interaction: Interaction,
}

blit::builder! {
    pub struct Resize<'a, C: Context> {
        new(state: &'a mut ResizeState<C::Scalar>),
        content<W = fn(Ui<'_, C>)>: W = |_| (),
        #[bounds(FnMut(Ui<'_, C>, ResizeGrip))]
        grip<F = fn(Ui<'_, C>, ResizeGrip)>: F = |_, _| (),
        initial: Size<C::Scalar> = Size::ZERO,
        minimum: Size<C::Scalar> = Size::ZERO,
        maximum: Size<C::Scalar> = Size::uniform(C::Scalar::UNBOUNDED),
        grip_size: Size<C::Scalar> = Size::uniform(C::Scalar::from_f32(1.0)),
    }
}

impl<C: Context, W, F> Widget<C> for Resize<'_, C, W, F>
where
    W: Widget<C>,
    F: FnMut(Ui<'_, C>, ResizeGrip),
{
    type Response = ();
    fn build(self, mut ui: Ui<'_, C>) {
        let state = self.state;
        let content = self.content;
        let mut grip = self.grip;
        let id = ui.current_id();
        let right_id = id.child("right grip");
        let bottom_id = id.child("bottom grip");
        let corner_id = id.child("corner grip");
        let previous = ui.area(id);
        let right = ui.interact_widget(right_id, Press::new());
        let bottom = ui.interact_widget(bottom_id, Press::new());
        let corner = ui.interact_widget(corner_id, Press::new());
        let mut drag = |id| {
            let drag = ui.interact_widget(id, Drag::new());
            drag.map_or(Point::ZERO, |drag| drag.delta)
        };
        let (right_delta, bottom_delta, corner_delta) = (drag(right_id), drag(bottom_id), drag(corner_id));
        let delta = Size::new(right_delta.x + corner_delta.x, bottom_delta.y + corner_delta.y);
        if delta != Size::ZERO {
            let size = state
                .size
                .or_else(|| previous.map(|area| area.size()))
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
                .id(grip_id)
                .build(|ui: Ui<'_, C>| grip(ui, ResizeGrip { edge, interaction }));
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
