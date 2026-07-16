//! fully resolved paint operations and their renderer-facing options

mod box_shadow;
mod image;
mod rectangle;
mod text;

pub use box_shadow::BoxShadow;
pub use image::{ImageFit, ImageRequest, ImageSampling, ImageTiling, NineSlice};
pub use rectangle::{Border, BorderRadius, GradientStop, LinearGradient, Rectangle};
pub use text::{
    FontId, HorizontalAlign, TextOptions, TextOverflow, TextRequest, TextStyle, TextWrap, VerticalAlign,
};
