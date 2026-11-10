use blit::{LogicalRect, PhysicalRect};
use blit_arena::Arena;
use blit_gui::{
    display_list::{DisplayList, Slice},
    image::{ImageData, ImageHandle},
};

pub use crate::render::rounded;
use crate::{
    Pixel, PixelBuffer, StoredImage,
    damage::region::Region,
    render::{self, image, image_patch},
    strategy::clip::ClipStack,
    text::TextRenderer,
};

pub struct DrawContext<'a, P> {
    pub scale: f32,
    /// physical drawing bounds independent of the current span
    pub bounds: PhysicalRect,
    pub(crate) list: &'a DisplayList,
    pub(crate) text: &'a mut TextRenderer,
    pub(crate) gradients: &'a mut render::fill::Gradients,
    pub(crate) images: &'a mut Arena<StoredImage>,
    pub(crate) source: Option<&'a Surface<P>>,
    pub(crate) target: DrawTarget<'a, P>,
}

impl<'a, P: Pixel> DrawContext<'a, P> {
    /// prepares an image with an explicit logical to physical scale
    pub fn prepare_image(&self, area: LogicalRect, request: &blit_gui::Image, scale: f32) -> Option<Image> {
        Image::new(area, request, scale, self.bounds, self.images)
    }

    pub fn slice<T: Copy + 'static>(&self, slice: Slice<T>) -> &'a [T] {
        self.list.slice(slice)
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        let id = self
            .images
            .insert_with(|id| StoredImage::new(ImageHandle::new(id.into(), data.size), data));
        self.images.get(id).unwrap().handle.clone()
    }

    pub fn source(&self) -> Option<&'a Surface<P>> {
        self.source
    }

    pub fn output_regions(&self) -> impl Iterator<Item = PhysicalRect> + '_ {
        let (span, pass) = match &self.target {
            DrawTarget::Prepare => (None, None),
            DrawTarget::Span { x, y, pixels, .. } => (Some(PhysicalRect::new(*x, *y, pixels.len() as i32, 1)), None),
            DrawTarget::Pass { output, .. } => (None, Some((*output, self.bounds))),
        };
        span.into_iter().chain(
            pass.into_iter()
                .flat_map(|(output, bounds)| output.rects().filter_map(move |region| region.intersection(bounds))),
        )
    }

    pub fn for_each_span(&mut self, mut draw: impl FnMut(Span<'_, P>)) {
        match &mut self.target {
            DrawTarget::Prepare => {}
            DrawTarget::Span { x, y, pixels, coverage } => draw(Span {
                x: *x,
                y: *y,
                pixels,
                coverage: *coverage,
                list: self.list,
                images: self.images,
            }),
            DrawTarget::Pass {
                target,
                clips,
                clip,
                output,
            } => {
                for bounds in output.rects().filter_map(|region| region.intersection(self.bounds)) {
                    for y in bounds.y..bounds.y + bounds.height {
                        clips.for_each(*clip, y, bounds.x..bounds.x + bounds.width, |range, coverage| {
                            target.process_line(y as usize, range.start as usize..range.end as usize, &mut |pixels| {
                                draw(Span {
                                    x: range.start,
                                    y,
                                    coverage,
                                    list: self.list,
                                    images: self.images,
                                    pixels,
                                })
                            });
                        });
                    }
                }
            }
        }
    }
}

pub struct Span<'a, P> {
    pub x: i32,
    pub y: i32,
    pub pixels: &'a mut [P],
    /// clip coverage to apply when blending pixels
    pub coverage: u8,
    pub(crate) list: &'a DisplayList,
    pub(crate) images: &'a Arena<StoredImage>,
}

impl<'a, P> Span<'a, P> {
    pub fn slice<T: Copy + 'static>(&self, slice: Slice<T>) -> &'a [T] {
        self.list.slice(slice)
    }

    pub fn image(&self, image: &ImageHandle) -> Option<&'a ImageData> {
        self.images.get(image.id().into()).map(|image| &image.data)
    }
}

pub struct Surface<P> {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) pixels: Vec<P>,
    pub(crate) valid: Region,
}

impl<P> Surface<P> {
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn row(&self, y: usize, range: std::ops::Range<usize>) -> &[P] {
        assert!(range.start <= range.end && range.end <= self.width && y < self.height);
        assert!(
            self.valid
                .contains(PhysicalRect::new(range.start as i32, y as i32, range.len() as i32, 1)),
            "pass read outside its declared source footprint"
        );
        &self.pixels[y * self.width + range.start..y * self.width + range.end]
    }
}

/// prepared image patches that retain their source image
pub struct Image {
    image: ImageHandle,
    patches: [Option<(image_patch::Prepared, PhysicalRect)>; 9],
}

impl Image {
    pub(crate) fn new(
        area: LogicalRect,
        request: &blit_gui::Image,
        scale: f32,
        bounds: PhysicalRect,
        images: &blit_arena::Arena<StoredImage>,
    ) -> Option<Self> {
        let texture = images.get(request.image.into())?;
        let mut patches = [None; 9];
        let mut count = 0;
        image::prepare(area, request, &texture.data, bounds, scale, |image, bounds| {
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
            if span.y < bounds.y || span.y >= bounds.y + bounds.height {
                continue;
            }
            let Some(clip) = bounds.intersection(row) else {
                continue;
            };
            let covered;
            let image = if span.coverage == 255 {
                image
            } else {
                covered = {
                    let mut image = *image;
                    image.opacity = (image.opacity as u16 * span.coverage as u16 / 255) as u8;
                    image
                };
                &covered
            };
            image.draw_line(span.pixels, &texture.data, &texture.alpha_rows, clip, span.x, span.y);
        }
    }
}

pub(crate) enum DrawTarget<'a, P> {
    Prepare,
    Span {
        x: i32,
        y: i32,
        pixels: &'a mut [P],
        coverage: u8,
    },
    Pass {
        target: &'a mut dyn Target<P>,
        clips: &'a ClipStack,
        clip: u32,
        output: &'a Region,
    },
}

pub(crate) trait Target<P> {
    fn process_line(&mut self, y: usize, range: std::ops::Range<usize>, draw: &mut dyn FnMut(&mut [P]));
}

impl<B: PixelBuffer> Target<B::Pixel> for B {
    fn process_line(&mut self, y: usize, range: std::ops::Range<usize>, draw: &mut dyn FnMut(&mut [B::Pixel])) {
        PixelBuffer::process_line(self, y, range, |pixels| draw(pixels));
    }
}
