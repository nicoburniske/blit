use crate::{LogicalRect, LogicalSize, PhysicalRect, SizedComponent, Ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageData<'a> {
    Rgb8(&'a [u8]),
    Rgba8(&'a [u8]),
    Rgba8Premultiplied(&'a [u8]),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageFit {
    #[default]
    Fill,
    Contain,
    Cover,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageSampling {
    Nearest,
    #[default]
    Bilinear,
}

crate::component! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Image<'a> {
        pub area: LogicalRect,
        pub data: ImageData<'a> = ImageData::Rgba8(&[]),
        pub width: usize,
        pub height: usize,
        pub stride_bytes: usize,
        pub fit: ImageFit,
        pub sampling: ImageSampling,
        pub opacity: f32 = 1.0,
    }
    features: []
}

impl<'a> Image<'a> {
    pub fn new(data: ImageData<'a>, width: usize, height: usize) -> Self {
        let (pixels, bytes_per_pixel) = match data {
            ImageData::Rgb8(pixels) => (pixels, 3),
            ImageData::Rgba8(pixels) | ImageData::Rgba8Premultiplied(pixels) => (pixels, 4),
        };
        let stride_bytes = width
            .checked_mul(bytes_per_pixel)
            .expect("image width is too large");
        assert!(
            height
                .checked_mul(stride_bytes)
                .is_some_and(|len| len <= pixels.len())
        );
        Self {
            data,
            width,
            height,
            stride_bytes,
            ..Self::default()
        }
    }

    pub fn render(self, ui: &mut Ui) {
        let mut clips = [PhysicalRect::default(); 8];
        let mut clip_count = 0;
        for dirty in ui.dirty.regions() {
            if let Some(clip) = self
                .area
                .to_physical(ui.scale_factor)
                .intersection(*dirty)
                .and_then(|area| area.intersection(ui.clip))
            {
                clips[clip_count] = clip;
                clip_count += 1;
            }
        }
        if clip_count != 0 {
            ui.platform().draw_image(&self, &clips[..clip_count]);
        }
    }
}

impl SizedComponent for Image<'_> {
    type Output = ();

    fn measure(&self, _: &mut Ui, available: LogicalRect) -> LogicalSize {
        let height = if self.width == 0 {
            0.0
        } else {
            available.width * self.height as f32 / self.width as f32
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
