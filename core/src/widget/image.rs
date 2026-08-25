use super::Widget;
use crate::{
    Ui,
    color::Color,
    container::{Item, Sizing},
    geometry::LogicalSize,
    image::{ImageContent, ImageFit, ImageHandle, ImageSampling, ImageTiling, NineSlice},
    node::Content,
};

crate::builder! {
    pub struct Image<'a> {
        new(resource: &'a ImageHandle),
        fit: ImageFit = ImageFit::default(),
        sampling: ImageSampling = ImageSampling::default(),
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
        z_index: i16 = 0,
        opacity: f32 = 1.0,
        colorize: Option<Color> = None,
        nine_slice: Option<NineSlice> = None,
        horizontal_tiling: ImageTiling = ImageTiling::default(),
        vertical_tiling: ImageTiling = ImageTiling::default(),
    }
}

impl Widget for Image<'_> {
    type Output = ();

    fn render(self, ui: &mut Ui) {
        let size = self.resource.size();
        ui.add_leaf(
            Item {
                width: self.width,
                height: self.height,
                z_index: self.z_index,
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
