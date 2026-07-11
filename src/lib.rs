mod layout;
mod platform;
mod rect;
#[cfg(test)]
mod test;
mod text;
pub mod widgets;

pub use layout::{Constraint, Direction, Layout};
pub use platform::{Platform, PlatformVTable};
pub use rect::Rect;
pub use text::{
    FontId, FontWeight, HorizontalAlign, Point, Text, TextMetrics, TextOptions, TextOverflow,
    TextRequest, TextStyle, TextWrap, VerticalAlign,
};
pub use tiny_skia::{Color, Pixmap, PixmapMut};

#[derive(Clone, Debug, Default)]
pub struct DirtyRegions {
    regions: [Rect; 8],
    len: usize,
}

impl DirtyRegions {
    pub fn add(&mut self, mut area: Rect) {
        if area.width <= 0.0 || area.height <= 0.0 {
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
            let mut growth = f32::INFINITY;
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

    pub fn regions(&self) -> &[Rect] {
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
        x: f32,
        y: f32,
    },
    PointerUp {
        x: f32,
        y: f32,
    },
    Char(char),
    Backspace,
}

pub struct Runtime {
    pixels: Pixmap,
    platform: Platform,
    screen: Rect,
    pending: DirtyRegions,
    previous: DirtyRegions,
}

impl Runtime {
    pub fn new(pixels: Pixmap, platform: Platform) -> Self {
        let screen = Rect {
            x: 0.0,
            y: 0.0,
            width: pixels.width() as f32,
            height: pixels.height() as f32,
        };
        let mut pending = DirtyRegions::default();
        pending.add(screen);
        Self {
            pixels,
            platform,
            screen,
            pending,
            previous: DirtyRegions::default(),
        }
    }

    pub fn render<R>(&mut self, input: Input, render: impl FnOnce(&mut Ui) -> R) -> R {
        let pending = std::mem::take(&mut self.pending);
        let mut dirty = std::mem::take(&mut self.previous);
        dirty.extend(&pending);
        self.previous = pending;
        // safety: Ui is private to this call and dropped before the framebuffer is returned
        let pixels = unsafe {
            std::mem::transmute::<PixmapMut<'_>, PixmapMut<'static>>(self.pixels.as_mut())
        };
        let mut ui = Ui {
            pixels,
            platform: self.platform,
            input,
            screen: self.screen,
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

    pub fn invalidate(&mut self, area: Rect) {
        self.pending.add(area)
    }

    pub fn invalidate_all(&mut self) {
        self.pending.add(self.screen)
    }

    pub fn screen(&self) -> Rect {
        self.screen
    }

    pub fn framebuffer(&self) -> &Pixmap {
        &self.pixels
    }
}

pub struct Ui {
    pixels: PixmapMut<'static>,
    platform: Platform,
    input: Input,
    screen: Rect,
    dirty: DirtyRegions,
    invalidated: DirtyRegions,
}

impl Ui {
    pub fn input(&self) -> &Input {
        &self.input
    }

    pub fn fill_rect(&mut self, area: Rect, color: Color) {
        use tiny_skia::{Paint, Rect as SkRect, Transform};

        let pixels = &mut self.pixels;
        let mut paint = Paint::default();
        paint.set_color(color);
        for dirty in self.dirty.regions() {
            let Some(area) = area.intersection(*dirty) else {
                continue;
            };
            pixels.fill_rect(
                SkRect::from_xywh(area.x, area.y, area.width, area.height).unwrap(),
                &paint,
                Transform::identity(),
                None,
            );
        }
    }

    pub fn invalidate(&mut self, area: Rect) {
        self.invalidated.add(area)
    }

    pub fn invalidate_all(&mut self) {
        self.invalidated.add(self.screen)
    }
}
