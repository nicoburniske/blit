#![no_std]

extern crate alloc;

pub mod blur;
pub mod clear;
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

use crate::text::system::TextSystem;

pub trait Context: blit::Context<Scalar = f32> + blit::HasFrameStore {
    fn text(&mut self) -> &mut TextSystem;
}
