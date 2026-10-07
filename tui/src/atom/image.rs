use blit::{Atom, Constraints, PhysicalRect, Size};

use crate::{
    TuiContext,
    image::{ImageId, ImagePlacement},
};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Image {
        new(image: ImageId, intrinsic: Size<i32>),
    }
}

impl Atom<TuiContext> for Image {
    fn measure(&self, _: &mut TuiContext, constraints: Constraints<i32>) -> Size<i32> {
        constraints.constrain(self.intrinsic)
    }

    fn paint(&self, context: &mut TuiContext, area: PhysicalRect) {
        context.place_image(ImagePlacement::new(self.image, area));
    }

    fn paint_bounds(&self, area: PhysicalRect) -> PhysicalRect {
        area
    }
}
