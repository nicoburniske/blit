use blit::PhysicalRect;

#[derive(Default)]
pub struct Region {
    screen: PhysicalRect,
    shift: u32,
    columns: usize,
    tiles: Vec<bool>,
}

impl Clone for Region {
    fn clone(&self) -> Self {
        let mut region = Self::default();
        region.clone_from(self);
        region
    }

    fn clone_from(&mut self, other: &Self) {
        self.screen = other.screen;
        self.shift = other.shift;
        self.columns = other.columns;
        self.tiles.clone_from(&other.tiles);
    }
}

impl Region {
    pub fn reset(&mut self, screen: PhysicalRect, shift: u32) {
        self.screen = screen;
        self.shift = shift;
        let size = 1usize << shift;
        self.columns = (screen.width as usize).div_ceil(size);
        self.tiles
            .resize(self.columns * (screen.height as usize).div_ceil(size), false);
        self.tiles.fill(false);
    }

    pub fn add(&mut self, bounds: PhysicalRect) {
        let Some(bounds) = bounds.intersection(self.screen) else {
            return;
        };
        let left = bounds.x as usize >> self.shift;
        let right = ((bounds.x + bounds.width - 1) as usize >> self.shift) + 1;
        let top = bounds.y as usize >> self.shift;
        let bottom = ((bounds.y + bounds.height - 1) as usize >> self.shift) + 1;
        for y in top..bottom {
            self.tiles[y * self.columns + left..y * self.columns + right].fill(true);
        }
    }

    pub fn intersects(&self, bounds: PhysicalRect) -> bool {
        let Some(bounds) = bounds.intersection(self.screen) else {
            return false;
        };
        let left = bounds.x as usize >> self.shift;
        let right = ((bounds.x + bounds.width - 1) as usize >> self.shift) + 1;
        let top = bounds.y as usize >> self.shift;
        let bottom = ((bounds.y + bounds.height - 1) as usize >> self.shift) + 1;
        (top..bottom).any(|y| {
            self.tiles[y * self.columns + left..y * self.columns + right]
                .iter()
                .any(|tile| *tile)
        })
    }

    pub fn contains(&self, bounds: PhysicalRect) -> bool {
        if bounds.width <= 0 || bounds.height <= 0 {
            return true;
        }
        if bounds.intersection(self.screen) != Some(bounds) {
            return false;
        }
        let left = bounds.x as usize >> self.shift;
        let right = ((bounds.x + bounds.width - 1) as usize >> self.shift) + 1;
        let top = bounds.y as usize >> self.shift;
        let bottom = ((bounds.y + bounds.height - 1) as usize >> self.shift) + 1;
        (top..bottom).all(|y| {
            self.tiles[y * self.columns + left..y * self.columns + right]
                .iter()
                .all(|tile| *tile)
        })
    }

    pub fn union(&mut self, other: &Self) {
        for (tile, other) in self.tiles.iter_mut().zip(&other.tiles) {
            *tile |= other;
        }
    }

    pub fn subtract(&mut self, other: &Self) {
        for (tile, other) in self.tiles.iter_mut().zip(&other.tiles) {
            *tile &= !other;
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.tiles.iter().any(|tile| *tile)
    }

    pub fn rects(&self) -> impl Iterator<Item = PhysicalRect> + '_ {
        let mut next = 0;
        std::iter::from_fn(move || {
            while next < self.tiles.len() && !self.tiles[next] {
                next += 1;
            }
            if next == self.tiles.len() {
                return None;
            }
            let start = next;
            let row_end = (start / self.columns + 1) * self.columns;
            while next < row_end && self.tiles[next] {
                next += 1;
            }
            let x = ((start % self.columns) << self.shift) as i32;
            let y = ((start / self.columns) << self.shift) as i32;
            Some(
                PhysicalRect::new(x, y, ((next - start) << self.shift) as i32, 1 << self.shift)
                    .intersection(self.screen)
                    .unwrap(),
            )
        })
    }

    pub fn line_ranges(&self, y: i32, bounds: PhysicalRect, ranges: &mut Vec<std::ops::Range<usize>>) -> i32 {
        ranges.clear();
        let Some(bounds) = bounds.intersection(self.screen) else {
            return self.screen.height;
        };
        if y < bounds.y {
            return bounds.y;
        }
        if y >= bounds.y + bounds.height {
            return self.screen.height;
        }
        let row = y as usize >> self.shift;
        let mut x = bounds.x as usize >> self.shift;
        let end = ((bounds.x + bounds.width - 1) as usize >> self.shift) + 1;
        while x < end {
            if !self.tiles[row * self.columns + x] {
                x += 1;
                continue;
            }
            let start = x;
            while x < end && self.tiles[row * self.columns + x] {
                x += 1;
            }
            ranges.push(
                (start << self.shift).max(bounds.x as usize)..(x << self.shift).min((bounds.x + bounds.width) as usize),
            );
        }
        (((row + 1) << self.shift) as i32).min(bounds.y + bounds.height)
    }
}
