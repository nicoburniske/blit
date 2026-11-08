use std::ops::Range;

use blit::{LogicalRect, PhysicalRect};
use blit_gui::style::BorderRadius;

use super::{DamageTracker, region};

/// command fingerprints and clip geometry used by damage trackers
#[derive(Default)]
pub struct Frame {
    pub records: Vec<Record>,
    pub clips: Vec<Clip>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub bounds: PhysicalRect,
    pub clip: u32,
    pub fingerprint: u64,
}

#[derive(Clone, Copy)]
pub struct Clip {
    pub parent: u32,
    pub area: LogicalRect,
    pub radius: BorderRadius,
}

impl Frame {
    pub fn clear(&mut self) {
        self.records.clear();
        self.clips.clear();
    }

    pub fn equivalent(&self, index: usize, other: &Self, other_index: usize) -> bool {
        let left = self.records[index];
        let right = other.records[other_index];
        if left.bounds != right.bounds || left.fingerprint != right.fingerprint {
            return false;
        }
        let mut clip = left.clip;
        let mut other_clip = right.clip;
        loop {
            match (clip, other_clip) {
                (0, 0) => return true,
                (0, _) | (_, 0) => return false,
                _ => {}
            }
            let left = &self.clips[clip as usize - 1];
            let right = &other.clips[other_clip as usize - 1];
            if left.area != right.area || left.radius != right.radius {
                return false;
            }
            clip = left.parent;
            other_clip = right.parent;
        }
    }
}

pub struct FrameDamage<'a, D> {
    pub tracker: &'a mut D,
    pub region: Option<&'a region::Region>,
}

impl<D: DamageTracker> DamageTracker for FrameDamage<'_, D> {
    fn update(&mut self, list: &Frame, screen: PhysicalRect, scale: f32, age: u8) {
        self.tracker.update(list, screen, scale, age);
    }

    fn invalidate(&mut self) {
        self.tracker.invalidate();
    }

    fn is_empty(&self) -> bool {
        self.region
            .map_or_else(|| self.tracker.is_empty(), region::Region::is_empty)
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

    fn finish_frame(&mut self, list: &mut Frame) {
        self.tracker.finish_frame(list);
    }
}
