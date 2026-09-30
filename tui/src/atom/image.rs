use blit::{Atom, IntrinsicQuery, IntrinsicSize, LogicalRect, LogicalSize};

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
    fn intrinsic(&self, _: &mut TuiContext, query: IntrinsicQuery) -> IntrinsicSize {
        IntrinsicSize {
            min: 0.0,
            preferred: query.axis.extent(self.intrinsic),
        }
    }

    fn paint(&self, context: &mut TuiContext, area: LogicalRect) {
        context.place_image(ImagePlacement::new(self.image, area));
    }
}
