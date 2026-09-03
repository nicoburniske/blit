use blit::{Atom, Constraints, LogicalRect, Size};
use blit_tui_render::image::{ImageId, ImagePlacement};

use crate::TuiPlatform;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Image {
        new(image: ImageId, intrinsic: Size),
    }
}

impl Atom<TuiPlatform> for Image {
    fn measure(&self, _: &mut TuiPlatform, constraints: Constraints) -> Size {
        constraints.constrain(self.intrinsic)
    }

    fn paint(&self, platform: &mut TuiPlatform, area: LogicalRect) {
        platform.place_image(ImagePlacement::new(self.image, area));
    }

    fn measure_depends_on_constraints(&self) -> bool {
        false
    }
}
