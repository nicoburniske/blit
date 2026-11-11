use std::ops::Range;

use blit::PhysicalRect;

pub mod command;
pub mod tile;

use crate::display_list::DisplayList;

pub trait DamageTracker {
    fn update(&mut self, input: &DisplayList, screen: PhysicalRect, scale: f32, buffer_age: u8);
    fn invalidate(&mut self);
    fn is_empty(&self) -> bool;
    fn intersects(&self, bounds: PhysicalRect, clip: u32) -> bool;
    /// writes sorted disjoint spans valid until the returned vertical boundary
    fn line_ranges(&self, line: i32, bounds: PhysicalRect, ranges: &mut Vec<Range<usize>>) -> i32;
    /// may swap the rendered list for one the renderer clears and reuses
    fn finish_frame(&mut self, _rendered: &mut DisplayList) {}
}
