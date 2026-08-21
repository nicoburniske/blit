use super::Widget;
use crate::{
    Content, ImageContent, Item, Sizing, Ui,
    color::Color,
    geometry::LogicalSize,
    paint::{ImageFit, ImageSampling, ImageTiling, NineSlice},
    resource::ImageHandle,
};

crate::builder! {
    pub struct Image<'a> {
        new(resource: &'a ImageHandle),
        fit: ImageFit = ImageFit::default(),
        sampling: ImageSampling = ImageSampling::default(),
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
        opacity: f32 = 1.0,
        colorize: Option<Color> = None,
        nine_slice: Option<NineSlice> = None,
        horizontal_tiling: ImageTiling = ImageTiling::default(),
        vertical_tiling: ImageTiling = ImageTiling::default(),
    }
}

impl Widget for Image<'_> {
    type Output = ();

    fn build(self, ui: &mut Ui) {
        let size = self.resource.size();
        ui.add_leaf(
            Item {
                width: self.width,
                height: self.height,
            },
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
