pub mod clip;
pub mod command;
mod direct;
mod line;
mod raster;

use blit::geometry::PhysicalRect;
pub use direct::Direct;
pub use line::Scanline;

use crate::{PixelBuffer, RenderContext};

pub trait RenderStrategy<B: PixelBuffer> {
    /// damage may overlap.
    /// render every covered pixel once, with conservative overdraw allowed
    fn render(&mut self, context: &mut RenderContext<B>, damage: &[PhysicalRect]);
}
