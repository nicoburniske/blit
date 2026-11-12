pub mod blur;
pub mod clear;
pub mod clip;
pub mod color;
pub mod damage;
pub mod display_list;
pub mod image;
pub mod mesh;
pub mod path;
pub mod rectangle;
pub mod shadow;
pub mod style;
pub mod text;

use blit::LogicalRect;

use crate::{style::BorderRadius, text::system::TextSystem};

pub trait Context: blit::Context<Scalar = f32> + blit::HasFrameStore {
    fn text(&mut self) -> &mut TextSystem;
    fn push_clip(&mut self, area: LogicalRect, radius: BorderRadius);
    fn pop_clip(&mut self);
}
