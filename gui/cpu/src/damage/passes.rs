use std::ops::Range;

use blit::{PhysicalRect, Scale2};
use blit_gui::{damage::DamageTracker, display_list::DisplayList};

use super::region::Region;
use crate::{PixelBuffer, RenderContext};

#[derive(Default)]
pub struct PassDamage {
    pub passes: Vec<PassNode>,
    pub stages: Vec<Region>,
    pub present: Region,
    pub preserve: Region,
    clips: Vec<PhysicalRect>,
    ranges: Vec<std::ops::Range<usize>>,
}

#[derive(Default)]
pub struct PassNode {
    pub index: u32,
    pub bounds: PhysicalRect,
    pub reads: Region,
}

impl PassDamage {
    pub fn update<B: PixelBuffer>(
        &mut self,
        context: &RenderContext<B>,
        tracker: &impl DamageTracker,
        screen: PhysicalRect,
        shift: u32,
    ) {
        let commands = &context.raster;
        let input = &context.list;
        if commands.passes.is_empty() || tracker.is_empty() {
            self.passes.clear();
            self.stages.clear();
            return;
        }
        let scale = context.scale_factor;
        self.clips.clear();
        self.clips.push(screen);
        for clip in input.clips() {
            self.clips.push(
                clip.area
                    .to_physical(Scale2::uniform(scale))
                    .intersection(self.clips[clip.parent as usize])
                    .unwrap_or_default(),
            );
        }
        let mut count = 0;
        for &index in &commands.passes {
            let Some(bounds) = commands
                .bounds(index)
                .intersection(self.clips[commands.clip(index) as usize])
            else {
                continue;
            };
            if count == self.passes.len() {
                self.passes.push(PassNode::default());
            }
            let node = &mut self.passes[count];
            node.index = index;
            node.bounds = bounds;
            node.reads.reset(screen, shift);
            count += 1;
        }
        self.passes.truncate(count);
        if count == 0 {
            self.stages.clear();
            return;
        }
        self.present.reset(screen, shift);
        let mut y = 0;
        while y < screen.height {
            let end = tracker.line_ranges(y, screen, &mut self.ranges);
            for range in &self.ranges {
                self.present
                    .add(PhysicalRect::new(range.start as i32, y, range.len() as i32, end - y));
            }
            y = end;
        }
        let size = 1i32 << shift;
        for node in &mut self.passes {
            let call = &commands.entries[node.index as usize];
            let mut y = node.bounds.y & !(size - 1);
            while y < node.bounds.y + node.bounds.height {
                let mut x = node.bounds.x & !(size - 1);
                while x < node.bounds.x + node.bounds.width {
                    let output = PhysicalRect::new(x, y, size, size).intersection(node.bounds).unwrap();
                    if self.present.intersects(call.reads(input.data(), output)) {
                        node.reads.add(output);
                    }
                    x += size;
                }
                y += size;
            }
            self.present.union(&node.reads);
        }
        self.stages.resize_with(count + 1, Region::default);
        self.stages[count].clone_from(&self.present);
        for index in (0..count).rev() {
            let node = &mut self.passes[index];
            node.reads.reset(screen, shift);
            let call = &commands.entries[node.index as usize];
            for output in self.stages[index + 1]
                .rects()
                .filter_map(|bounds| bounds.intersection(node.bounds))
            {
                node.reads.add(call.reads(input.data(), output));
            }
            let (before, after) = self.stages.split_at_mut(index + 1);
            before[index].clone_from(&after[0]);
            before[index].union(&node.reads);
        }
        self.preserve.clone_from(&self.stages[0]);
        self.preserve.subtract(&self.present);
    }
}

pub struct StageDamage<'a, D> {
    pub tracker: &'a mut D,
    pub region: Option<&'a Region>,
}

impl<D: DamageTracker> DamageTracker for StageDamage<'_, D> {
    fn update(&mut self, input: &DisplayList, screen: PhysicalRect, scale: f32, age: u8) {
        self.tracker.update(input, screen, scale, age);
    }

    fn invalidate(&mut self) {
        self.tracker.invalidate();
    }

    fn is_empty(&self) -> bool {
        self.region.map_or_else(|| self.tracker.is_empty(), Region::is_empty)
    }

    fn intersects(&self, bounds: PhysicalRect, clip: u32) -> bool {
        match self.region {
            Some(region) => region.intersects(bounds),
            None => self.tracker.intersects(bounds, clip),
        }
    }

    fn line_ranges(&self, line: i32, bounds: PhysicalRect, ranges: &mut Vec<Range<usize>>) -> i32 {
        let Some(region) = self.region else {
            return self.tracker.line_ranges(line, bounds, ranges);
        };
        region.line_ranges(line, bounds, ranges)
    }

    fn finish_frame(&mut self, input: &mut DisplayList) {
        self.tracker.finish_frame(input);
    }
}
