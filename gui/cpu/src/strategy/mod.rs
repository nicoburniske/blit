pub mod clip;
pub mod command;
mod direct;
mod line;
mod raster;

pub use self::{direct::Direct, line::Scanline};
use crate::{DamageTracker, PixelBuffer, RenderState};

pub trait RenderStrategy<B: PixelBuffer> {
    /// render each dirty pixel once in drawing order
    fn render(&mut self, context: &mut RenderState<B>, damage: &impl DamageTracker);
}
