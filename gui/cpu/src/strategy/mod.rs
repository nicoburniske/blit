pub mod clip;
pub mod command;
mod direct;
mod line;
mod raster;

pub use self::{direct::Direct, line::Scanline};
use crate::{DamageTracker, PixelBuffer, RenderContext};

pub trait RenderStrategy<B: PixelBuffer> {
    /// render each dirty pixel once in drawing order
    fn render<H>(&mut self, context: &mut RenderContext<B, H>, damage: &impl DamageTracker);
}
