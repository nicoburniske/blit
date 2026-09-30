use blit::{Atom, Constraints, LogicalRect, LogicalSize};

use crate::{
    TuiContext,
    image::{ImageId, ImagePlacement},
};

blit::builder! {
    #[const]
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Image {
        new(image: ImageId, intrinsic: LogicalSize),
    }
}

impl Atom<TuiContext> for Image {
    fn measure(&self, _: &mut TuiContext, constraints: Constraints) -> LogicalSize {
        constraints.constrain(self.intrinsic)
    }

    fn paint(&self, context: &mut TuiContext, area: LogicalRect) {
        context.place_image(ImagePlacement::new(self.image, area));
    }

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        area
    }
}
