use blit::{Animation, Axis, Easing, Id, Layout, Motion, Scalar, Sides, Size, Spring, Tween, Ui, Widget, state::Open};
use blit_widget::{
    Context, interact,
    layout::{Align, Justify, Sizing, absolute, absolute::Anchor, flex},
};

/// applies selection changes before building any option's visuals
pub fn select<'a, C: Context, T: Copy + PartialEq, D>(
    selected: &'a mut T,
    options: &'a [(D, T)],
    layout: impl Layout<C>,
    mut render: impl FnMut(Ui<'_, C>, &'a D, bool, interact::press::Response),
) -> impl Widget<C, Response = bool> {
    move |mut ui: Ui<'_, C>| {
        let mut clicked = false;
        let items = ui.scope();
        for (index, (_, value)) in options.iter().enumerate() {
            let item = ui.resolve(items, index);
            let press = ui.node(item).interact(interact::press());
            if press.clicked {
                *selected = *value;
                clicked = true;
            }
        }
        let mut group = ui.layout(layout);
        for (index, (data, value)) in options.iter().enumerate() {
            group.child_in(items, index).build(|mut ui: Ui<'_, C>| {
                let press = ui.interact(interact::press());
                render(ui, data, *selected == *value, press);
            });
        }
        clicked
    }
}

/// named columns of cards that move by drag and drop
pub fn board<'a, C: Context>(
    columns: &'a mut [(&'static str, Vec<u32>)],
    height: C::Scalar,
    gap: C::Scalar,
    mut decorate: impl FnMut(&mut Ui<'_, C, Open<flex::Layout<C::Scalar>>>, &str) + 'a,
    mut card: impl FnMut(Ui<'_, C>, Option<u32>, bool) + 'a,
) -> impl Widget<C> + 'a {
    move |mut ui: Ui<'_, C>| {
        let spring = Spring::new(0.350, 0.15);
        let item = || flex::item().height(height);
        // cards are keyed in the board rather than their column so they keep their state across columns
        let (lanes, tiles) = (ui.scope(), ui.scope());
        let mut board = ui.layout(flex::row().gap(gap).align(Align::Stretch));
        let mut dragged = None;
        for (index, (name, cards)) in columns.iter().enumerate() {
            let mut column = board
                .child_in(lanes, index)
                .item(flex::item().grow())
                .layout(flex::column().gap(gap).align(Align::Stretch))
                .pad(gap);
            decorate(&mut column, name);
            for &id in cards {
                let mut node = column.child_in(tiles, id);
                let response = node.interact(interact::drag());
                if response.active {
                    dragged = Some((id, response.position));
                }
                // it stays lifted on the release frame and lands from there the next
                let Some(area) = (response.active || response.released).then(|| node.area()).flatten() else {
                    node.item(item())
                        .motion(Motion::new(spring).position().lift(Id::ROOT))
                        .build(|ui: Ui<'_, C>| card(ui, Some(id), false));
                    continue;
                };
                node.place(
                    Id::ROOT,
                    absolute::place()
                        .x(area.x + response.delta.x)
                        .y(area.y + response.delta.y)
                        .width(area.width)
                        .height(area.height),
                )
                .parent(Id::ROOT)
                .z_index(1)
                // no properties so it follows the pointer exactly and keeps its motion state
                .motion(Motion::new(spring))
                .build(|ui: Ui<'_, C>| card(ui, Some(id), true));
                // the gap where it lands
                column.child().item(item()).build(|ui: Ui<'_, C>| card(ui, None, false));
            }
        }
        // the dragged card moves to the column under the pointer after the other cards above the pointer
        if let Some((id, pointer)) = dragged
            && let Some(column) = (0..columns.len()).position(|index| {
                let lane = board.resolve(lanes, index);
                board.node(lane).area().is_some_and(|area| area.contains(pointer))
            })
        {
            let row = columns[column]
                .1
                .iter()
                .filter(|&&other| other != id)
                .filter(|&&other| {
                    let tile = board.resolve(tiles, other);
                    board
                        .node(tile)
                        .area()
                        .is_some_and(|area| area.y.lerp(area.y + area.height, 0.5) < pointer.y)
                })
                .count();
            for (_, cards) in columns.iter_mut() {
                cards.retain(|&other| other != id);
            }
            columns[column].1.insert(row, id);
        }
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transition {
    Off,
    #[default]
    Spring,
    Tween,
}

impl Transition {
    pub fn motion(self) -> Option<Motion> {
        let duration = 0.25;
        let animation = match self {
            Self::Off => return None,
            Self::Spring => Animation::Spring(Spring::new(duration, 0.2).rest(0.01, 0.01)),
            Self::Tween => Animation::Tween(Tween::new(duration, Easing::EASE_IN_OUT)),
        };
        Some(Motion::new(animation).layout())
    }
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
    pub transition: Transition,
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
            transition: Transition::Spring,
        }
    }
}

impl CanvasConfig {
    pub fn padding<T: Scalar>(self, unit: Size<T>) -> Sides<T> {
        let steps = f32::from(self.padding_steps) * self.zoom;
        Sides::xy(
            T::from_f32(steps * unit.width.to_f32()),
            T::from_f32(steps * unit.height.to_f32()),
        )
    }

    pub fn gap<T: Scalar>(self, axis: Axis, unit: Size<T>) -> T {
        T::from_f32(f32::from(self.gap_steps) * self.zoom * axis.extent(unit).to_f32())
    }

    pub fn item_sizing<T: Scalar>(self, index: usize, unit: Size<T>) -> (Sizing<T>, Sizing<T>) {
        let main_steps = 3.0 + (index % 5) as f32;
        let cross_steps = 3.0 + (index % 4) as f32;
        let main_unit = self.axis.extent(unit).to_f32();
        let cross_unit = self.axis.other().extent(unit).to_f32();
        let natural_main = T::from_f32(main_steps * main_unit * self.zoom);
        let natural_cross = T::from_f32(cross_steps * cross_unit * self.zoom);
        let main = match self.sizing {
            ItemSizing::Fixed => Sizing::fixed(natural_main),
            ItemSizing::Fit => Sizing::fit_range(T::from_f32(2.0 * main_unit), natural_main),
            ItemSizing::Grow => Sizing::grow_range(T::from_f32(2.0 * main_unit), T::UNBOUNDED),
        };
        let cross = if self.align == Align::Stretch {
            Sizing::fit()
        } else {
            Sizing::fixed(natural_cross)
        };
        match self.axis {
            Axis::Horizontal => (main, cross),
            Axis::Vertical => (cross, main),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemSpec {
    pub label: &'static str,
    pub rows: u32,
    pub columns: u32,
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
