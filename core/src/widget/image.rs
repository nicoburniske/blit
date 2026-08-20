use super::{SizedWidget, Widget};
use crate::{
    Content, Element, ImageContent, Layout, Ui,
    color::Color,
    geometry::{LogicalRect, LogicalSize},
    layout::Constraints,
    paint::{ImageFit, ImageRequest, ImageSampling, ImageTiling, NineSlice},
    resource::ImageHandle,
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

    fn measure(&self, _: &mut Ui, constraints: Constraints) -> LogicalSize {
        let size = self.resource.size();
        if size.width == 0 || size.height == 0 {
            return constraints.constrain(LogicalSize::default());
        }
        let width = size.width as f32;
        let height = size.height as f32;
        let scale = 1.0f32
            .min(constraints.max.width / width)
            .min(constraints.max.height / height)
            .max(constraints.min.width / width)
            .max(constraints.min.height / height);
        constraints.constrain(LogicalSize {
            width: width * scale,
            height: height * scale,
        })
    }

    fn render(self, ui: &mut Ui, area: LogicalRect) -> Self::Output {
        Image::render(self, ui, area)
    }
}

impl Widget for Image<'_> {
    type Output = ();

    fn build(self, ui: &mut Ui) {
        if self.resource.is_empty() {
            return;
        }
        let size = self.resource.size();
        drop(
            ui.element(
                Element::new(Layout::horizontal()).content(Content::Image(ImageContent {
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
                })),
            ),
        );
    }
}
