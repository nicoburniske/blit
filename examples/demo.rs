use std::time::Duration;

use blit::{Animation, Axis, Easing, Id, Layout, Motion, Point, Scalar, Sides, Size, Spring, Tween, Ui, Widget};
pub use blit_layout::{Align, Justify};
use blit_layout::{Sizing, absolute::Anchor};
use blit_widgets::{Context, interact};

/// applies selection changes before building any option's visuals
pub fn select<'a, C: Context, T: Copy + PartialEq, D>(
    selected: &'a mut T,
    options: &'a [(D, T)],
    layout: impl Layout<C>,
    mut render: impl FnMut(Ui<'_, C>, &'a D, bool, interact::press::Response),
) -> impl Widget<C, Response = bool> {
    move |mut ui: Ui<'_, C>| {
        let id = ui.current_id();
        let mut clicked = false;
        for (index, (_, value)) in options.iter().enumerate() {
            let press = ui.interact_widget(id.child(index), interact::press());
            if press.clicked {
                *selected = *value;
                clicked = true;
            }
        }
        let mut group = ui.layout(layout);
        for (index, (data, value)) in options.iter().enumerate() {
            group.child().id(id.child(index)).build(|mut ui: Ui<'_, C>| {
                let press = ui.interact(interact::press());
                render(ui, data, *selected == *value, press);
            });
        }
        clicked
    }
}

pub struct SpringDrag {
    pub spring: Spring,
    pub active: bool,
    position: [f32; 2],
}

impl Default for SpringDrag {
    fn default() -> Self {
        Self {
            spring: Spring::new(Duration::from_millis(650), 0.5).rest(0.01, 0.01),
            active: false,
            position: [0.0; 2],
        }
    }
}

impl SpringDrag {
    pub fn update<C: Context, S>(&mut self, ui: &mut Ui<'_, C, S>, id: Id) -> Point<C::Scalar> {
        let drag = ui.interact_widget(id, interact::drag());
        let card = ui.area(id);
        let home = ui.area(Id::new((id, "home")));
        self.active = drag.active;
        if drag.activated
            && let (Some(card), Some(home)) = (card, home)
        {
            self.position = [(card.x - home.x).to_f32(), (card.y - home.y).to_f32()];
        }
        if drag.active {
            self.position[0] += drag.delta.x.to_f32();
            self.position[1] += drag.delta.y.to_f32();
        } else {
            self.position = [0.0; 2];
        }
        Point::new(
            C::Scalar::from_f32(self.position[0]),
            C::Scalar::from_f32(self.position[1]),
        )
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
        let duration = Duration::from_millis(250);
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
