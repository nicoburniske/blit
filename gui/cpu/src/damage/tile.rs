//! tile damage tracking inspired by [rxi's cached software rendering](https://rxi.github.io/cached_software_rendering.html)
//!
//! each render hashes command contents and relevant clips in drawing order into the
//! physical tiles they intersect. comparing tile hashes detects changes without
//! retaining the previous command data
//!
//! per tile ages count presentations since the last change. tiles younger than the
//! supplied buffer age need repainting. buffer age zero forces a full repaint
//!
//! a summed area table supports constant time damage queries over command bounds
//! rendering consumes disjoint horizontal runs of dirty tiles

use std::{hash::Hash, ops::Range};

use blit::{PhysicalRect, Scale2, internal::hash::FxHasher};

use super::{DamageTracker, DisplayList};

pub struct TileDamage {
    shift: u32,
    screen: PhysicalRect,
    columns: usize,
    previous: Vec<u64>,
    ages: Vec<u8>,
    hashes: Vec<u64>,
    sums: Vec<u32>,
    clips: Vec<Clip>,
    invalidated: bool,
}

struct Clip {
    hash: u64,
    bounds: PhysicalRect,
    inner: PhysicalRect,
}

impl TileDamage {
    pub fn new(tile_size: u16) -> Self {
        assert!(
            tile_size.is_power_of_two(),
            "damage tile size must be a nonzero power of two"
        );
        Self {
            shift: tile_size.trailing_zeros(),
            screen: PhysicalRect::default(),
            columns: 0,
            previous: Vec::new(),
            ages: Vec::new(),
            hashes: Vec::new(),
            sums: vec![0],
            clips: Vec::new(),
            invalidated: true,
        }
    }
}

impl DamageTracker for TileDamage {
    fn update(
        &mut self,
        input: &DisplayList<crate::raster::Operations>,
        screen: PhysicalRect,
        scale: f32,
        buffer_age: u8,
    ) {
        let tile_size = 1usize << self.shift;
        if self.screen != screen {
            self.screen = screen;
            self.columns = (screen.width as usize).div_ceil(tile_size);
            let rows = (screen.height as usize).div_ceil(tile_size);
            let count = self.columns.checked_mul(rows).expect("damage grid is too large");
            u32::try_from(count).expect("too many damage tiles");
            self.previous.resize(count, 0);
            self.ages.resize(count, 0);
            self.hashes.resize(count, 0);
            let sums = (self.columns + 1)
                .checked_mul(rows + 1)
                .expect("damage grid is too large");
            self.sums.resize(sums, 0);
            self.sums.fill(0);
            self.invalidated = true;
        }
        self.hashes.fill(0);
        self.clips.clear();
        self.clips.push(Clip {
            hash: 0,
            bounds: screen,
            inner: screen,
        });
        for clip in input.clips() {
            let parent = &self.clips[clip.parent as usize];
            let area = clip.area.to_physical(Scale2::uniform(scale));
            let radius = [
                clip.radius.top_left,
                clip.radius.top_right,
                clip.radius.bottom_right,
                clip.radius.bottom_left,
            ]
            .into_iter()
            .fold(0.0f32, f32::max);
            let inset = (radius * scale).ceil() as i32;
            let inner = PhysicalRect::new(
                area.x.saturating_add(inset),
                area.y.saturating_add(inset),
                area.width.saturating_sub(inset.saturating_mul(2)).max(0),
                area.height.saturating_sub(inset.saturating_mul(2)).max(0),
            );
            let mut hash = FxHasher::default();
            hash.write_u64(parent.hash);
            [
                clip.area.x,
                clip.area.y,
                clip.area.width,
                clip.area.height,
                clip.radius.top_left,
                clip.radius.top_right,
                clip.radius.bottom_right,
                clip.radius.bottom_left,
            ]
            .map(f32::to_bits)
            .hash(&mut hash);
            self.clips.push(Clip {
                hash: hash.finish(),
                bounds: area.intersection(parent.bounds).unwrap_or_default(),
                inner: inner.intersection(parent.inner).unwrap_or_default(),
            });
        }
        for (index, record) in input.commands().iter().enumerate() {
            let clip = &self.clips[record.clip as usize];
            let Some(bounds) = record.bounds.intersection(clip.bounds) else {
                continue;
            };
            let mut hash = FxHasher::default();
            [bounds.x, bounds.y, bounds.width, bounds.height].hash(&mut hash);
            if bounds.intersection(clip.inner) != Some(bounds) {
                hash.write_u64(clip.hash);
            }
            input.hash(index, &mut hash);
            let hash = hash.finish();
            let left = bounds.x as usize >> self.shift;
            let right = ((bounds.x + bounds.width - 1) as usize >> self.shift) + 1;
            let top = bounds.y as usize >> self.shift;
            let bottom = ((bounds.y + bounds.height - 1) as usize >> self.shift) + 1;
            for row in top..bottom {
                for cell in &mut self.hashes[row * self.columns + left..row * self.columns + right] {
                    *cell = cell.rotate_left(5).wrapping_add(hash).wrapping_mul(0xf1357aea2e62a9c5);
                }
            }
        }
        let stride = self.columns + 1;
        for (row, ((previous, hashes), ages)) in self
            .previous
            .chunks_mut(self.columns.max(1))
            .zip(self.hashes.chunks(self.columns.max(1)))
            .zip(self.ages.chunks_mut(self.columns.max(1)))
            .enumerate()
        {
            let mut count = 0;
            for (column, ((previous, hash), age)) in previous.iter_mut().zip(hashes).zip(ages).enumerate() {
                if self.invalidated || *previous != *hash {
                    *previous = *hash;
                    *age = 0;
                } else {
                    *age = age.saturating_add(1);
                }
                count += u32::from(buffer_age == 0 || *age < buffer_age);
                self.sums[(row + 1) * stride + column + 1] = self.sums[row * stride + column + 1] + count;
            }
        }
        self.invalidated = false;
    }

    fn invalidate(&mut self) {
        self.invalidated = true;
    }

    fn is_empty(&self) -> bool {
        self.sums.last() == Some(&0)
    }

    /// conservatively tests bounds against damage and their command clip
    fn intersects(&self, bounds: PhysicalRect, clip: u32) -> bool {
        let Some(bounds) = bounds.intersection(self.clips[clip as usize].bounds) else {
            return false;
        };
        let left = bounds.x as usize >> self.shift;
        let right = ((bounds.x + bounds.width - 1) as usize >> self.shift) + 1;
        let top = bounds.y as usize >> self.shift;
        let bottom = ((bounds.y + bounds.height - 1) as usize >> self.shift) + 1;
        let stride = self.columns + 1;
        self.sums[bottom * stride + right] - self.sums[top * stride + right]
            != self.sums[bottom * stride + left] - self.sums[top * stride + left]
    }

    /// writes sorted disjoint horizontal ranges and returns their next vertical boundary
    fn line_ranges(&self, line: i32, bounds: PhysicalRect, ranges: &mut Vec<Range<usize>>) -> i32 {
        ranges.clear();
        let mut next = bounds.y.saturating_add(bounds.height);
        let Some(bounds) = bounds.intersection(self.screen) else {
            return next;
        };
        if line < bounds.y || line >= bounds.y + bounds.height {
            return next;
        }
        let row = line as usize >> self.shift;
        let size = 1usize << self.shift;
        next = next.min(((row + 1) * size).min(self.screen.height as usize) as i32);
        let left = bounds.x as usize >> self.shift;
        let right = ((bounds.x + bounds.width - 1) as usize >> self.shift) + 1;
        let stride = self.columns + 1;
        let mut start = None;
        for column in left..right {
            let above = row * stride + column;
            let below = above + stride;
            let dirty = self.sums[below + 1] - self.sums[above + 1] != self.sums[below] - self.sums[above];
            if dirty {
                start.get_or_insert((column * size).max(bounds.x as usize));
            } else if let Some(start) = start.take() {
                ranges.push(start..column * size);
            }
        }
        if let Some(start) = start {
            ranges.push(start..(bounds.x + bounds.width) as usize);
        }
        next
    }
}
