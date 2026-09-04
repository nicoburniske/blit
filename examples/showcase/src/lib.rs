use std::{collections::VecDeque, time::Duration};

use blit::{Anchor, Axis, Sides, Size, Sizing};
pub use blit_std::layout::{Align, Justify};

#[derive(Debug, Default)]
pub struct FpsCounter {
    frame_at: Option<Duration>,
    updated_at: Option<Duration>,
    frames: VecDeque<Duration>,
}

impl FpsCounter {
    pub fn update(&mut self, now: Duration) -> Option<f32> {
        if self.frame_at.replace(now) == Some(now) {
            return None;
        }
        self.frames.push_back(now);
        while self
            .frames
            .front()
            .is_some_and(|frame| now.saturating_sub(*frame) > Duration::from_secs(1))
        {
            self.frames.pop_front();
        }
        if self
            .updated_at
            .is_none_or(|updated| now.saturating_sub(updated) >= Duration::from_millis(250))
        {
            self.updated_at = Some(now);
            if let (Some(first), Some(last)) = (self.frames.front(), self.frames.back())
                && self.frames.len() > 1
            {
                let elapsed = last.saturating_sub(*first).as_secs_f32();
                return Some(self.frames.len().saturating_sub(1) as f32 / elapsed);
            }
        }
        None
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
    pub fn padding(self, unit: Size) -> Sides {
        let steps = f32::from(self.padding_steps) * self.zoom;
        Sides {
            top: steps * unit.height,
            right: steps * unit.width,
            bottom: steps * unit.height,
            left: steps * unit.width,
        }
    }

    pub fn gap(self, axis: Axis, unit: Size) -> f32 {
        let unit = match axis {
            Axis::Horizontal => unit.width,
            Axis::Vertical => unit.height,
        };
        f32::from(self.gap_steps) * self.zoom * unit
    }

    pub fn item_sizing(self, index: usize, unit: Size) -> (Sizing, Sizing) {
        let main_steps = 3.0 + (index % 5) as f32;
        let cross_steps = 3.0 + (index % 4) as f32;
        let (main_unit, cross_unit) = match self.axis {
            Axis::Horizontal => (unit.width, unit.height),
            Axis::Vertical => (unit.height, unit.width),
        };
        let natural_main = main_steps * main_unit * self.zoom;
        let natural_cross = cross_steps * cross_unit * self.zoom;
        let main = match self.sizing {
            ItemSizing::Fixed => Sizing::fixed(natural_main),
            ItemSizing::Fit => Sizing::fit_range(2.0 * main_unit, natural_main),
            ItemSizing::Grow => Sizing::grow_range(2.0 * main_unit, f32::INFINITY),
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
    pub rows: usize,
    pub columns: usize,
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
