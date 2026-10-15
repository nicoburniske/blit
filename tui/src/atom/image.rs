use blit::{Atom, AtomCx, Constraints, PhysicalRect, Size};

use crate::{
    Context,
    image::{ImageId, ImagePlacement},
};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Image {
        new(image: ImageId, intrinsic: Size<i32>),
    }
}

blit::atom_widget!(Image);

impl<C: Context> Atom<C> for Image {
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<i32>) -> Size<i32> {
        constraints.constrain(self.intrinsic)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: PhysicalRect) {
        cx.context_mut()
            .as_mut()
            .place_image(ImagePlacement::new(self.image, area));
    }
}
