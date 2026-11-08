use std::ops::Range;

use blit::PhysicalRect;

pub mod command;
mod frame;
pub(crate) mod passes;
pub(crate) mod region;
pub mod tile;

pub(crate) use frame::FrameDamage;
pub use frame::{Clip, Frame, Record};

pub trait DamageTracker {
    fn update(&mut self, frame: &Frame, screen: PhysicalRect, scale: f32, buffer_age: u8);
    fn invalidate(&mut self);
    fn is_empty(&self) -> bool;
    fn intersects(&self, bounds: PhysicalRect, clip: u32) -> bool;
    /// writes sorted disjoint spans valid until the returned vertical boundary
    fn line_ranges(&self, line: i32, bounds: PhysicalRect, ranges: &mut Vec<Range<usize>>) -> i32;
    fn finish_frame(&mut self, _frame: &mut Frame) {}
}
