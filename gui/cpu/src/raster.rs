use blit::PhysicalRect;
use blit_gui::image::{ImageHandle, ImageRequest};

pub use crate::render::{blur, rounded};
use crate::{
    DrawContext, PaintContext, Pixel, Span, StoredImage,
    render::{image, image_patch},
};

/// prepared image patches that retain their source image
pub struct Image {
    image: ImageHandle,
    patches: [Option<(image_patch::Prepared, PhysicalRect)>; 9],
}

impl<H: 'static, P: Pixel> PaintContext<'_, H, P> {
    /// prepares an image with an explicit logical to physical scale
    pub fn prepare_image(&self, request: &ImageRequest, scale: f32) -> Option<Image> {
        Image::new(request, scale, self.bounds, self.images)
    }
}

impl<P: Pixel> DrawContext<'_, P> {
    /// prepares an image with an explicit logical to physical scale
    pub fn prepare_image(&self, request: &ImageRequest, scale: f32) -> Option<Image> {
        Image::new(request, scale, self.bounds, self.images)
    }
}

impl Image {
    fn new(
        request: &ImageRequest,
        scale: f32,
        bounds: PhysicalRect,
        images: &blit_arena::Arena<StoredImage>,
    ) -> Option<Self> {
        let texture = images.get(request.image.into())?;
        let mut patches = [None; 9];
        let mut count = 0;
        image::prepare(request, &texture.data, bounds, scale, |image, bounds| {
            patches[count] = Some((image, bounds));
            count += 1;
        });
        (count != 0).then(|| Self {
            image: texture.handle.clone(),
            patches,
        })
    }

    #[inline]
    pub fn draw<P: Pixel>(&self, span: &mut Span<'_, P>) {
        let texture = span.images.get(self.image.id().into()).unwrap();
        let row = PhysicalRect::new(span.x, span.y, span.pixels.len() as i32, 1);
        for (image, bounds) in self.patches.iter().flatten() {
            let Some(clip) = bounds.intersection(row) else {
                continue;
            };
            let mut image = *image;
            image.opacity = (image.opacity as u16 * span.coverage as u16 / 255) as u8;
            image.draw_line(span.pixels, &texture.data, &texture.alpha_rows, clip, span.x, span.y);
        }
    }
}
