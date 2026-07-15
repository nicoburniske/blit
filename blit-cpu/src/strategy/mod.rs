pub(crate) mod clip;
pub(crate) mod command;
mod direct;
mod line;
mod raster;

pub use direct::Direct;
pub use line::Scanline;

use blit::PhysicalRect;

use crate::{PixelBuffer, RenderContext};

pub trait RenderStrategy<B: PixelBuffer> {
    fn render(&mut self, context: &mut RenderContext<B>, damage: &[PhysicalRect]);
}
