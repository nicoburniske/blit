use blit::{FrameSlice, LogicalRect, PhysicalRect, ScratchBuffer};
use blit_arena::Arena;
use blit_gui::{
    display_list::DisplayList,
    image::{ImageData, ImageHandle},
};

use crate::{
    Pixel, PixelBuffer, StoredImage,
    damage::region::Region,
    raster::{self, image, image_patch},
    strategy::clip::ClipStack,
    text::TextRenderer,
};

pub struct RenderContext<'a, P> {
    pub scale: f32,
    /// physical command bounds before clipping
    pub bounds: PhysicalRect,
    pub(crate) list: &'a DisplayList,
    pub(crate) text: &'a mut TextRenderer,
    pub(crate) gradients: &'a mut raster::fill::Gradients,
    pub(crate) images: &'a mut Arena<StoredImage>,
    pub(crate) source: Option<&'a Surface<P>>,
    pub(crate) scratch: &'a ScratchBuffer,
    pub(crate) target: RenderTarget<'a, P>,
}

impl<'a, P: Pixel> RenderContext<'a, P> {
    /// prepares an image with an explicit logical to physical scale
    pub fn prepare_image(&self, area: LogicalRect, request: &blit_gui::Image, scale: f32) -> Option<Image> {
        Image::new(area, request, scale, self.bounds, self.images)
    }

    pub fn slice<T: Copy + 'static>(&self, slice: FrameSlice<T>) -> &'a [T] {
        self.list.slice(slice)
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        let id = self.images.insert_with(|id| StoredImage::new(id.into(), data));
        self.images.get(id).unwrap().handle.clone()
    }

    pub fn source(&self) -> Option<&'a Surface<P>> {
        self.source
    }

    pub fn scratch(&self) -> &'a ScratchBuffer {
        self.scratch
    }

    /// required output rectangles before precise clip coverage
    pub fn output_regions(&self) -> impl Iterator<Item = PhysicalRect> + 'a {
        let (span, pass) = match &self.target {
            RenderTarget::Prepare => (None, None),
            RenderTarget::Span { x, y, pixels, .. } => (Some(PhysicalRect::new(*x, *y, pixels.len() as i32, 1)), None),
            RenderTarget::Pass {
                output, clips, clip, ..
            } => (None, clips.bounds(*clip, self.bounds).map(|bounds| (*output, bounds))),
        };
        span.into_iter().chain(
            pass.into_iter()
                .flat_map(|(output, bounds)| output.rects().filter_map(move |region| region.intersection(bounds))),
        )
    }

    pub fn for_each_span(&mut self, draw: impl FnMut(Span<'_, P>)) {
        self.for_each_span_in(self.bounds, draw);
    }

    /// visits clipped output spans inside a physical rectangle
    pub fn for_each_span_in(&mut self, area: PhysicalRect, mut draw: impl FnMut(Span<'_, P>)) {
        let Some(area) = area.intersection(self.bounds) else {
            return;
        };
        match &mut self.target {
            RenderTarget::Prepare => {}
            RenderTarget::Span { x, y, pixels, coverage } => {
                let Some(bounds) = PhysicalRect::new(*x, *y, pixels.len() as i32, 1).intersection(area) else {
                    return;
                };
                let start = (bounds.x - *x) as usize;
                draw(Span {
                    x: bounds.x,
                    y: *y,
                    pixels: &mut pixels[start..start + bounds.width as usize],
                    coverage: *coverage,
                    list: self.list,
                    images: self.images,
                });
            }
            RenderTarget::Pass {
                target,
                clips,
                clip,
                output,
            } => {
                for bounds in output.rects().filter_map(|region| region.intersection(area)) {
                    for y in bounds.y..bounds.y + bounds.height {
                        clips.for_each(*clip, y, bounds.x..bounds.x + bounds.width, |range, coverage| {
                            draw(Span {
                                x: range.start,
                                y,
                                coverage,
                                list: self.list,
                                images: self.images,
                                pixels: target.write_line(y as usize, range.start as usize..range.end as usize),
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
    pub fn slice<T: Copy + 'static>(&self, slice: FrameSlice<T>) -> &'a [T] {
        self.list.slice(slice)
    }

    pub fn image(&self, image: &ImageHandle) -> Option<&'a ImageData> {
        self.images.get(image.id().into()).map(|image| &image.data)
    }
}

pub struct Surface<P> {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) bounds: PhysicalRect,
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
        if range.is_empty() {
            return &[];
        }
        let start = (y - self.bounds.y as usize) * self.bounds.width as usize + range.start - self.bounds.x as usize;
        &self.pixels[start..start + range.len()]
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
        let texture = images.get(request.image.id().into())?;
        let mut patches = [None; 9];
        let mut count = 0;
        image::prepare(area, request, bounds, scale, |image, bounds| {
            patches[count] = Some((image, bounds));
            count += 1;
        });
        (count != 0).then(|| Self {
            image: texture.handle.clone(),
            patches,
        })
    }

    #[inline]
    pub fn render<P: Pixel>(&self, span: &mut Span<'_, P>) {
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

pub(crate) enum RenderTarget<'a, P> {
    Prepare,
    Span {
        x: i32,
        y: i32,
        pixels: &'a mut [P],
        coverage: u8,
    },
    Pass {
        target: &'a mut dyn PixelBuffer<Pixel = P>,
        clips: &'a ClipStack,
        clip: u32,
        output: &'a Region,
    },
}
