use std::ops::Range;

use blit::PhysicalRect;

pub mod command;
pub(crate) mod passes;
pub(crate) mod region;
pub mod tile;

use blit_gui::display_list::DisplayList;

pub trait DamageTracker {
    fn update(
        &mut self,
        input: &DisplayList<crate::raster::Operations>,
        screen: PhysicalRect,
        scale: f32,
        buffer_age: u8,
    );
    fn invalidate(&mut self);
    fn is_empty(&self) -> bool;
    fn intersects(&self, bounds: PhysicalRect, clip: u32) -> bool;
    /// writes sorted disjoint spans valid until the returned vertical boundary
    fn line_ranges(&self, line: i32, bounds: PhysicalRect, ranges: &mut Vec<Range<usize>>) -> i32;
    fn finish_frame(&mut self, _input: &mut DisplayList<crate::raster::Operations>) {}
}
