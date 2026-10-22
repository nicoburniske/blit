//! tile damage tracking inspired by [rxi's cached software rendering](https://rxi.github.io/cached_software_rendering.html)
//!
//! each frame hashes command contents and relevant clips in drawing order into the
//! physical tiles they intersect. comparing tile hashes detects changes without
//! retaining the previous display list
//!
//! per tile ages count presentations since the last change. tiles younger than the
//! supplied buffer age need repainting. buffer age zero forces a full repaint
//!
//! a summed area table supports constant time damage queries over command bounds
//! rendering consumes disjoint horizontal runs of dirty tiles

use std::{
    hash::{Hash, Hasher},
    ops::Range,
};

use blit::{PhysicalRect, Scale2, internal::hash::FxHasher};
use blit_gui::{
    display_list::{Command, DisplayList},
    style::Fill,
};

pub struct Damage {
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

impl Damage {
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

    pub fn update(&mut self, display_list: &DisplayList, screen: PhysicalRect, scale: f32, buffer_age: u8) {
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
        for clip in display_list.clips() {
            let parent = &self.clips[clip.parent.0 as usize];
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
        for record in display_list.iter() {
            let clip = &self.clips[record.clip.0 as usize];
            let Some(bounds) = record.bounds.intersection(clip.bounds) else {
                continue;
            };
            let mut hash = FxHasher::default();
            [bounds.x, bounds.y, bounds.width, bounds.height].hash(&mut hash);
            if bounds.intersection(clip.inner) != Some(bounds) {
                hash.write_u64(clip.hash);
            }
            std::mem::discriminant(&record.command).hash(&mut hash);
            match record.command {
                Command::Clear => {}
                Command::Rectangle(rectangle) => {
                    [
                        rectangle.area.x,
                        rectangle.area.y,
                        rectangle.area.width,
                        rectangle.area.height,
                        rectangle.radius.top_left,
                        rectangle.radius.top_right,
                        rectangle.radius.bottom_right,
                        rectangle.radius.bottom_left,
                        rectangle.opacity,
                        rectangle.border.width,
                    ]
                    .map(f32::to_bits)
                    .hash(&mut hash);
                    for fill in [rectangle.background, rectangle.border.fill] {
                        std::mem::discriminant(&fill).hash(&mut hash);
                        match fill {
                            Fill::Solid(color) => color.hash(&mut hash),
                            Fill::LinearGradient(gradient) => {
                                hash.write_u32(gradient.angle_degrees.to_bits());
                                gradient.stops.len().hash(&mut hash);
                                for stop in gradient.stops {
                                    hash.write_u32(stop.position.to_bits());
                                    stop.color.hash(&mut hash);
                                }
                            }
                        }
                    }
                }
                Command::Text(text, colors) => {
                    text.text.hash(&mut hash);
                    [
                        text.area.x,
                        text.area.y,
                        text.area.width,
                        text.area.height,
                        text.offset_x,
                    ]
                    .map(f32::to_bits)
                    .hash(&mut hash);
                    text.color.hash(&mut hash);
                    text.options.wrap.hash(&mut hash);
                    text.options.overflow.hash(&mut hash);
                    text.options.horizontal_align.hash(&mut hash);
                    text.options.vertical_align.hash(&mut hash);
                    text.options.max_lines.hash(&mut hash);
                    colors.hash(&mut hash);
                }
                Command::Image(image) => {
                    hash.write_u64(image.image);
                    [
                        image.area.x,
                        image.area.y,
                        image.area.width,
                        image.area.height,
                        image.opacity,
                    ]
                    .map(f32::to_bits)
                    .hash(&mut hash);
                    std::mem::discriminant(&image.fit).hash(&mut hash);
                    std::mem::discriminant(&image.sampling).hash(&mut hash);
                    std::mem::discriminant(&image.horizontal_tiling).hash(&mut hash);
                    std::mem::discriminant(&image.vertical_tiling).hash(&mut hash);
                    image.colorize.hash(&mut hash);
                    image
                        .nine_slice
                        .map(|slice| [slice.top, slice.right, slice.bottom, slice.left])
                        .hash(&mut hash);
                }
                Command::BoxShadow(shadow) => {
                    [
                        shadow.area.x,
                        shadow.area.y,
                        shadow.area.width,
                        shadow.area.height,
                        shadow.radius.top_left,
                        shadow.radius.top_right,
                        shadow.radius.bottom_right,
                        shadow.radius.bottom_left,
                        shadow.offset_x,
                        shadow.offset_y,
                        shadow.blur,
                        shadow.spread,
                    ]
                    .map(f32::to_bits)
                    .hash(&mut hash);
                    shadow.color.hash(&mut hash);
                    shadow.inset.hash(&mut hash);
                }
                Command::Mesh(mesh) => {
                    mesh.vertices.len().hash(&mut hash);
                    for vertex in mesh.vertices {
                        [vertex.position.x, vertex.position.y].map(f32::to_bits).hash(&mut hash);
                        vertex.color.hash(&mut hash);
                    }
                    mesh.indices.hash(&mut hash);
                }
                _ => continue,
            }
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

    pub fn invalidate(&mut self) {
        self.invalidated = true;
    }

    pub fn is_empty(&self) -> bool {
        self.sums.last() == Some(&0)
    }

    /// conservatively tests bounds against damage and their display list clip
    pub fn intersects(&self, bounds: PhysicalRect, clip: u32) -> bool {
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
    pub fn line_ranges(&self, line: i32, bounds: PhysicalRect, ranges: &mut Vec<Range<usize>>) -> i32 {
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
