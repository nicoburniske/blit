use std::{collections::VecDeque, ops::Range};

use blit::{PhysicalRect, Scale2};

use super::{DamageTracker, Frame};

/// compares ordered command fingerprints and their clip chains
#[derive(Default)]
pub struct CommandDiff {
    previous: Frame,
    current: Vec<PhysicalRect>,
    history: VecDeque<Vec<PhysicalRect>>,
    damage: Vec<PhysicalRect>,
    clips: Vec<PhysicalRect>,
    screen: PhysicalRect,
    max_age: u8,
    initialized: bool,
}

impl DamageTracker for CommandDiff {
    fn update(&mut self, frame: &Frame, screen: PhysicalRect, scale: f32, buffer_age: u8) {
        self.max_age = self.max_age.max(buffer_age).max(1);
        self.current.clear();
        if !self.initialized || self.screen != screen {
            self.current.push(screen);
            self.history.clear();
            self.initialized = true;
        } else {
            let previous = &self.previous;
            let mut start = 0;
            let common = previous.records.len().min(frame.records.len());
            while start < common && previous.equivalent(start, frame, start) {
                start += 1;
            }
            let mut old_end = previous.records.len();
            let mut new_end = frame.records.len();
            while old_end > start && new_end > start && previous.equivalent(old_end - 1, frame, new_end - 1) {
                old_end -= 1;
                new_end -= 1;
            }
            let mut old = start;
            let mut new = start;
            while old < old_end && new < new_end {
                if previous.equivalent(old, frame, new) {
                    old += 1;
                    new += 1;
                    continue;
                }
                let mut aligned = false;
                for offset in 1..=8 {
                    if new + offset < new_end && previous.equivalent(old, frame, new + offset) {
                        for index in new..new + offset {
                            push_damage(&mut self.current, frame.records[index].bounds);
                        }
                        new += offset;
                        aligned = true;
                        break;
                    }
                    if old + offset < old_end && previous.equivalent(old + offset, frame, new) {
                        for index in old..old + offset {
                            push_damage(&mut self.current, previous.records[index].bounds);
                        }
                        old += offset;
                        aligned = true;
                        break;
                    }
                }
                if aligned {
                    continue;
                }
                let old_bounds = previous.records[old].bounds;
                let new_bounds = frame.records[new].bounds;
                push_damage(&mut self.current, old_bounds);
                if new_bounds != old_bounds {
                    push_damage(&mut self.current, new_bounds);
                }
                old += 1;
                new += 1;
            }
            for index in old..old_end {
                push_damage(&mut self.current, previous.records[index].bounds);
            }
            for index in new..new_end {
                push_damage(&mut self.current, frame.records[index].bounds);
            }
        }
        self.screen = screen;
        self.damage.clear();
        if buffer_age == 0 || self.history.len() < buffer_age.saturating_sub(1) as usize {
            self.damage.push(screen);
        } else {
            self.damage.extend_from_slice(&self.current);
            for damage in self.history.iter().take(buffer_age.saturating_sub(1) as usize) {
                self.damage.extend_from_slice(damage);
            }
        }
        self.clips.clear();
        self.clips.push(screen);
        for clip in &frame.clips {
            self.clips.push(
                clip.area
                    .to_physical(Scale2::uniform(scale))
                    .intersection(self.clips[clip.parent as usize])
                    .unwrap_or_default(),
            );
        }
    }

    fn invalidate(&mut self) {
        self.initialized = false;
    }

    fn is_empty(&self) -> bool {
        !self
            .damage
            .iter()
            .any(|bounds| bounds.intersection(self.screen).is_some())
    }

    fn intersects(&self, bounds: PhysicalRect, clip: u32) -> bool {
        let Some(bounds) = bounds.intersection(self.clips[clip as usize]) else {
            return false;
        };
        self.damage.iter().any(|damage| bounds.intersection(*damage).is_some())
    }

    fn line_ranges(&self, line: i32, bounds: PhysicalRect, ranges: &mut Vec<Range<usize>>) -> i32 {
        ranges.clear();
        let mut next = bounds.y.saturating_add(bounds.height);
        let Some(bounds) = bounds.intersection(self.screen) else {
            return next;
        };
        if line < bounds.y || line >= bounds.y + bounds.height {
            return next;
        }
        for damage in &self.damage {
            let Some(damage) = damage.intersection(bounds) else {
                continue;
            };
            if damage.y > line {
                next = next.min(damage.y);
            } else if line < damage.y + damage.height {
                next = next.min(damage.y + damage.height);
                ranges.push(damage.x as usize..(damage.x + damage.width) as usize);
            }
        }
        ranges.sort_unstable_by_key(|range| range.start);
        let mut count = 0;
        for index in 0..ranges.len() {
            if count > 0 && ranges[index].start <= ranges[count - 1].end {
                ranges[count - 1].end = ranges[count - 1].end.max(ranges[index].end);
            } else {
                ranges[count] = ranges[index].clone();
                count += 1;
            }
        }
        ranges.truncate(count);
        next
    }

    fn finish_frame(&mut self, frame: &mut Frame) {
        std::mem::swap(&mut self.previous, frame);
        self.history.push_front(std::mem::take(&mut self.current));
        if self.history.len() >= self.max_age as usize {
            self.current = self.history.pop_back().unwrap();
        }
    }
}

fn push_damage(damage: &mut Vec<PhysicalRect>, bounds: PhysicalRect) {
    if bounds.width <= 0 || bounds.height <= 0 {
        return;
    }
    if damage.len() == 32 {
        for index in 0..16 {
            damage[index] = damage[index * 2].union(damage[index * 2 + 1]);
        }
        damage.truncate(16);
    }
    damage.push(bounds);
}
