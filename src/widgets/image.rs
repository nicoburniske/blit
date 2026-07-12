use crate::{Color, ImageId, ImageResource, LogicalRect, LogicalSize, SizedComponent, Ui};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageFit {
    #[default]
    Fill,
    Contain,
    Cover,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageSampling {
    #[default]
    Nearest,
    Bilinear,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageTiling {
    #[default]
    None,
    Repeat,
    Round,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NineSlice {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl NineSlice {
    pub const fn uniform(value: u16) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageRequest {
    pub image: ImageId,
    pub area: LogicalRect,
    pub fit: ImageFit,
    pub sampling: ImageSampling,
    pub opacity: f32,
    pub colorize: Option<Color>,
    pub nine_slice: Option<NineSlice>,
    pub horizontal_tiling: ImageTiling,
    pub vertical_tiling: ImageTiling,
}

pub struct Image<'a> {
    pub resource: &'a ImageResource,
    pub area: LogicalRect,
    pub fit: ImageFit,
    pub sampling: ImageSampling,
    pub opacity: f32,
    pub colorize: Option<Color>,
    pub nine_slice: Option<NineSlice>,
    pub horizontal_tiling: ImageTiling,
    pub vertical_tiling: ImageTiling,
}

impl<'a> Image<'a> {
    pub fn new(resource: &'a ImageResource) -> Self {
        Self {
            resource,
            area: LogicalRect::default(),
            fit: ImageFit::default(),
            sampling: ImageSampling::default(),
            opacity: 1.0,
            colorize: None,
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::None,
        }
    }

    pub fn area(mut self, area: LogicalRect) -> Self {
        self.area = area;
        self
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn sampling(mut self, sampling: ImageSampling) -> Self {
        self.sampling = sampling;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    pub fn colorize(mut self, color: Color) -> Self {
        self.colorize = Some(color);
        self
    }

    pub fn nine_slice(mut self, nine_slice: NineSlice) -> Self {
        self.nine_slice = Some(nine_slice);
        self
    }

    pub fn horizontal_tiling(mut self, tiling: ImageTiling) -> Self {
        self.horizontal_tiling = tiling;
        self
    }

    pub fn vertical_tiling(mut self, tiling: ImageTiling) -> Self {
        self.vertical_tiling = tiling;
        self
    }

    pub fn render(self, ui: &mut Ui) {
        let request = ImageRequest {
            image: self.resource.id(),
            area: self.area,
            fit: self.fit,
            sampling: self.sampling,
            opacity: self.opacity,
            colorize: self.colorize,
            nine_slice: self.nine_slice,
            horizontal_tiling: self.horizontal_tiling,
            vertical_tiling: self.vertical_tiling,
        };
        ui.record_draw(request.area);
        if let Some(clip) = ui.draw_clip(request.area) {
            ui.platform().draw_image(&request, clip);
        }
    }
}

impl SizedComponent for Image<'_> {
    type Output = ();

    fn measure(&self, _: &mut Ui, available: LogicalRect) -> LogicalSize {
        let size = self.resource.size();
        let height = if size.width == 0 {
            0.0
        } else {
            available.width * size.height as f32 / size.width as f32
        }
        .min(available.height);
        LogicalSize {
            width: available.width,
            height,
        }
    }

    fn render(mut self, ui: &mut Ui, area: LogicalRect) -> Self::Output {
        self.area = area;
        Image::render(self, ui)
    }
}
