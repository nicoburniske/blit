use blit::{LogicalRect, PhysicalRect};
use blit_gui::{
    color::Color,
    image::{ImageHandle, ImageRequest},
    paint::Rectangle,
    style::BorderRadius,
};

pub use crate::render::{blur, rounded};
use crate::{
    PaintContext, Pixel, PixelSpan, Span,
    render::{fill::Gradients, image, image_patch, rectangle},
};

/// prepared image patches that retain their source image
pub struct Image {
    image: ImageHandle,
    patches: [Option<(image_patch::Prepared, PhysicalRect)>; 9],
}

/// a prepared solid rectangle with optional rounded corners
pub struct SolidRect(rectangle::Prepared);

impl<H: 'static, P: Pixel> PaintContext<'_, H, P> {
    /// prepares an image with an explicit logical to physical scale
    pub fn prepare_image(&self, request: &ImageRequest, scale: f32) -> Option<Image> {
        let texture = self.images.get(request.image.into())?;
        let mut patches = [None; 9];
        let mut count = 0;
        image::prepare(request, &texture.data, self.bounds, scale, |image, bounds| {
            patches[count] = Some((image, bounds));
            count += 1;
        });
        (count != 0).then(|| Image {
            image: texture.handle.clone(),
            patches,
        })
    }
}

impl Image {
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

impl SolidRect {
    pub fn new(area: LogicalRect, radius: BorderRadius, color: Color, scale: f32) -> Option<Self> {
        rectangle::Prepared::new(
            &Rectangle::new(area).background(color).radius(radius),
            scale,
            &mut Gradients::default(),
        )
        .map(Self)
    }

    #[inline]
    pub fn draw<P: Pixel>(&self, span: &mut Span<'_, P>) {
        self.0.draw_line(
            &Gradients::default(),
            span.y,
            PhysicalRect::new(span.x, span.y, span.pixels.len() as i32, 1),
            span.coverage,
            PixelSpan {
                x: span.x,
                pixels: span.pixels,
            },
        );
    }
}
