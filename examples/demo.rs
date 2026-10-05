use blit::{Axis, Scalar, Sides, Size};
pub use blit_layout::{Align, Justify};
use blit_layout::{Sizing, absolute::Anchor};

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
