use blit::{Atom, AtomCx, Constraints, PhysicalRect, Size};

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

blit::atom_widget!(Image);

impl Atom<TuiContext> for Image {
    fn measure(&self, _: &mut AtomCx<'_, TuiContext>, constraints: Constraints<i32>) -> Size<i32> {
        constraints.constrain(self.intrinsic)
    }

    fn paint(&self, cx: &mut AtomCx<'_, TuiContext>, area: PhysicalRect) {
        cx.context_mut().place_image(ImagePlacement::new(self.image, area));
    }
}
