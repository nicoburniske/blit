mod color;
mod layout;
mod platform;
mod rect;
#[cfg(test)]
mod test;
mod text;
pub mod widgets;

pub use color::Color;
pub use layout::{Constraint, Direction, Layout, RepeatedAreas, RepeatedLayout};
pub use platform::{Platform, PlatformImpl, PlatformVTable};
pub use rect::{LogicalInsets, LogicalRect, PhysicalRect};
pub use text::{
    FontId, FontWeight, HorizontalAlign, LogicalPoint, PhysicalPoint, Text, TextOptions,
    TextOverflow, TextRequest, TextStyle, TextWrap, VerticalAlign,
};

#[derive(Clone, Debug, Default)]
pub struct DirtyRegions {
    regions: [PhysicalRect; 8],
    len: usize,
}

impl DirtyRegions {
    pub fn add(&mut self, mut area: PhysicalRect) {
        if area.width <= 0 || area.height <= 0 {
            return;
        }

        loop {
            let mut index = 0;
            while index < self.len {
                if area.touches(self.regions[index]) {
                    area = area.union(self.regions[index]);
                    self.len -= 1;
                    self.regions[index] = self.regions[self.len];
                    index = 0;
                } else {
                    index += 1;
                }
            }

            if self.len < self.regions.len() {
                self.regions[self.len] = area;
                self.len += 1;
                return;
            }

            let mut best = 0;
            let mut growth = i64::MAX;
            for index in 0..self.len {
                let candidate = area.union(self.regions[index]);
                let candidate_growth = candidate.area() - self.regions[index].area();
                if candidate_growth < growth {
                    best = index;
                    growth = candidate_growth;
                }
            }
            area = area.union(self.regions[best]);
            self.len -= 1;
            self.regions[best] = self.regions[self.len];
        }
    }

    pub fn regions(&self) -> &[PhysicalRect] {
        &self.regions[..self.len]
    }

    pub fn extend(&mut self, other: &Self) {
        for area in other.regions() {
            self.add(*area);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Input {
    #[default]
    None,
    PointerDown {
        position: LogicalPoint,
    },
    PointerUp {
        position: LogicalPoint,
    },
    Char(char),
    Backspace,
}

pub struct Runtime {
    platform: Platform,
    screen: LogicalRect,
    physical_screen: PhysicalRect,
    scale_factor: f32,
    pending: DirtyRegions,
    previous: DirtyRegions,
}

impl Runtime {
    pub fn new(platform: Platform) -> Self {
        let physical_screen = platform.screen();
        let scale_factor = platform.scale_factor();
        assert!(scale_factor.is_finite() && scale_factor > 0.0);
        let screen = physical_screen.to_logical(scale_factor);
        let mut pending = DirtyRegions::default();
        pending.add(physical_screen);
        Self {
            platform,
            screen,
            physical_screen,
            scale_factor,
            pending,
            previous: DirtyRegions::default(),
        }
    }

    pub fn render<R>(&mut self, input: Input, render: impl FnOnce(&mut Ui) -> R) -> R {
        let pending = std::mem::take(&mut self.pending);
        let mut dirty = std::mem::take(&mut self.previous);
        dirty.extend(&pending);
        self.previous = pending;
        let mut ui = Ui {
            platform: self.platform,
            input,
            screen: self.screen,
            physical_screen: self.physical_screen,
            scale_factor: self.scale_factor,
            dirty,
            invalidated: DirtyRegions::default(),
        };
        let output = render(&mut ui);
        self.pending = ui.invalidated;
        output
    }

    pub fn has_pending_redraw(&self) -> bool {
        !self.pending.is_empty() || !self.previous.is_empty()
    }

    pub fn invalidate(&mut self, area: LogicalRect) {
        if let Some(area) = area
            .to_physical(self.scale_factor)
            .intersection(self.physical_screen)
        {
            self.pending.add(area)
        }
    }

    pub fn invalidate_all(&mut self) {
        self.pending.add(self.physical_screen)
    }

    pub fn screen(&self) -> LogicalRect {
        self.screen
    }
}

pub struct Ui {
    pub(crate) platform: Platform,
    input: Input,
    screen: LogicalRect,
    physical_screen: PhysicalRect,
    scale_factor: f32,
    dirty: DirtyRegions,
    invalidated: DirtyRegions,
}

impl Ui {
    pub fn input(&self) -> &Input {
        &self.input
    }

    pub fn invalidate(&mut self, area: LogicalRect) {
        if let Some(area) = area
            .to_physical(self.scale_factor)
            .intersection(self.physical_screen)
        {
            self.invalidated.add(area)
        }
    }

    pub fn invalidate_all(&mut self) {
        self.invalidated.add(self.physical_screen)
    }
}
