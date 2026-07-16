use super::SizedWidget;
use crate::{
    color::Color,
    geometry::{LogicalRect, LogicalSize},
    paint::{ImageFit, ImageRequest, ImageSampling, ImageTiling, NineSlice},
    resource::ImageHandle,
    Ui,
};

crate::widget! {
    pub struct Image<'a> {
        new(pub resource: &'a ImageHandle);
        pub fit: ImageFit,
        pub sampling: ImageSampling,
        pub opacity: f32 = 1.0,
        #[skip]
        pub colorize: Option<Color>,
        #[skip]
        pub nine_slice: Option<NineSlice>,
        pub horizontal_tiling: ImageTiling,
        pub vertical_tiling: ImageTiling,
    }
}

impl<'a> Image<'a> {
    pub fn colorize(mut self, color: Color) -> Self {
        self.colorize = Some(color);
        self
    }

    pub fn nine_slice(mut self, nine_slice: NineSlice) -> Self {
        self.nine_slice = Some(nine_slice);
        self
    }

    pub fn render(self, ui: &mut Ui, area: LogicalRect) {
        if self.resource.is_empty() {
            return;
        }
        let request = ImageRequest {
            image: self.resource.id(),
            area,
            fit: self.fit,
            sampling: self.sampling,
            opacity: self.opacity,
            colorize: self.colorize,
            nine_slice: self.nine_slice,
            horizontal_tiling: self.horizontal_tiling,
            vertical_tiling: self.vertical_tiling,
        };
        ui.paint_image(request);
    }
}

impl SizedWidget for Image<'_> {
    type Output = ();

    fn measure(&self, _: &mut Ui, available: LogicalRect) -> LogicalSize {
        let size = self.resource.size();
        let height =
            if size.width == 0 { 0.0 } else { available.width * size.height as f32 / size.width as f32 }
                .min(available.height);
        LogicalSize { width: available.width, height }
    }

    fn render(self, ui: &mut Ui, area: LogicalRect) -> Self::Output { Image::render(self, ui, area) }
}
