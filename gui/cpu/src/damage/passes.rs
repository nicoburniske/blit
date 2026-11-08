use blit::{PhysicalRect, Scale2};
use blit_gui::display_list::{Command, DisplayList};

use super::{DamageTracker, region::Region};
use crate::{Pixel, extension::registry::Registry};

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
    pub index: usize,
    pub handler: usize,
    pub bounds: PhysicalRect,
    pub reads: Region,
}

impl PassDamage {
    pub fn update<P: Pixel>(
        &mut self,
        list: &DisplayList,
        registry: &Registry<P>,
        tracker: &impl DamageTracker,
        screen: PhysicalRect,
        scale: f32,
        shift: u32,
    ) {
        if !list.has_custom_commands() {
            self.passes.clear();
            self.stages.clear();
            return;
        }
        self.clips.clear();
        self.clips.push(screen);
        for clip in list.clips() {
            self.clips.push(
                clip.area
                    .to_physical(Scale2::uniform(scale))
                    .intersection(self.clips[clip.parent.0 as usize])
                    .unwrap_or_default(),
            );
        }
        let mut count = 0;
        for (index, record) in list.iter().enumerate() {
            let Command::Custom(command) = record.command else {
                continue;
            };
            let handler = registry.index(command);
            if registry.entries[handler]
                .1
                .read_bounds(command, record.bounds, scale)
                .is_none()
            {
                continue;
            }
            let Some(bounds) = record.bounds.intersection(self.clips[record.clip.0 as usize]) else {
                continue;
            };
            if count == self.passes.len() {
                self.passes.push(PassNode::default());
            }
            let node = &mut self.passes[count];
            node.index = index;
            node.handler = handler;
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
            let Command::Custom(command) = list.get(node.index).command else {
                unreachable!()
            };
            let handler = &registry.entries[node.handler].1;
            let mut y = node.bounds.y & !(size - 1);
            while y < node.bounds.y + node.bounds.height {
                let mut x = node.bounds.x & !(size - 1);
                while x < node.bounds.x + node.bounds.width {
                    let output = PhysicalRect::new(x, y, size, size).intersection(node.bounds).unwrap();
                    if self
                        .present
                        .intersects(handler.read_bounds(command, output, scale).unwrap_or_default())
                    {
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
            let Command::Custom(command) = list.get(node.index).command else {
                unreachable!()
            };
            let handler = &registry.entries[node.handler].1;
            for output in self.stages[index + 1]
                .rects()
                .filter_map(|bounds| bounds.intersection(node.bounds))
            {
                node.reads
                    .add(handler.read_bounds(command, output, scale).unwrap_or_default());
            }
            let (before, after) = self.stages.split_at_mut(index + 1);
            before[index].clone_from(&after[0]);
            before[index].union(&node.reads);
        }
        self.preserve.clone_from(&self.stages[0]);
        self.preserve.subtract(&self.present);
    }
}
