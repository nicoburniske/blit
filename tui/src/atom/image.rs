use crate::image::{ImageId, ImagePlacement};
use blit::{Atom, Constraints, LogicalRect, Size};

use crate::TuiContext;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Image {
        new(image: ImageId, intrinsic: Size),
    }
}

impl Atom<TuiContext> for Image {
    fn measure(&self, _: &mut TuiContext, constraints: Constraints) -> Size {
        constraints.constrain(self.intrinsic)
    }

    fn paint(&self, context: &mut TuiContext, area: LogicalRect) {
        context.place_image(ImagePlacement::new(self.image, area));
    }

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        area
    }
}
