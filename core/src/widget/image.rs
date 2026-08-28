use super::Widget;
use crate::{
    Ui,
    color::Color,
    container::Slot,
    geometry::LogicalSize,
    image::{ImageContent, ImageFit, ImageHandle, ImageSampling, ImageTiling, NineSlice},
    node::Content,
};

crate::builder! {
    pub struct Image<'a> {
        new(resource: &'a ImageHandle),
        @optional {
            colorize: Color,
            nine_slice: NineSlice,
        },
        slot: Slot = Slot::new(),
        fit: ImageFit = ImageFit::default(),
        sampling: ImageSampling = ImageSampling::default(),
        opacity: f32 = 1.0,
        horizontal_tiling: ImageTiling = ImageTiling::default(),
        vertical_tiling: ImageTiling = ImageTiling::default(),
    }
}

impl Widget for Image<'_> {
    type Output = ();

    fn render(self, ui: &mut Ui) {
        let size = self.resource.size();
        ui.add_leaf(
            self.slot,
            Content::Image(ImageContent {
                image: self.resource.id(),
                intrinsic: LogicalSize {
                    width: size.width as f32,
                    height: size.height as f32,
                },
                fit: self.fit,
                sampling: self.sampling,
                opacity: self.opacity,
                colorize: self.colorize,
                nine_slice: self.nine_slice,
                horizontal_tiling: self.horizontal_tiling,
                vertical_tiling: self.vertical_tiling,
            }),
        );
    }
}
