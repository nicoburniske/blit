use blit::PhysicalRect;
use blit_arena::Arena as ImageArena;
use blit_gui::{
    display_list::{
        Custom, CustomCommand,
        arena::{Arena, Value, Writer},
    },
    image::{ImageData, ImageHandle},
};

use crate::{Pixel, PixelBuffer, StoredImage, damage::region::Region, strategy::clip::ClipStack};

/// selects the cpu handler for a custom command
///
/// unsupported commands cannot be recorded
///
/// ```compile_fail
/// use blit::{LogicalRect, Paint};
/// use blit_cpu::{Renderer, VecBuffer, Xrgb8888};
/// #[derive(PartialEq, Hash)]
/// struct Unsupported;
/// fn draw(renderer: &mut Renderer<VecBuffer<Xrgb8888>>) {
///     renderer.paint(LogicalRect::default(), Unsupported);
/// }
/// ```
///
/// arena builders require the same handler
///
/// ```compile_fail
/// use blit::LogicalRect;
/// use blit_cpu::{Renderer, VecBuffer, Xrgb8888};
/// use blit_gui::PaintCustom;
/// #[derive(PartialEq, Hash)]
/// struct Unsupported;
/// fn draw(renderer: &mut Renderer<VecBuffer<Xrgb8888>>) {
///     renderer.paint_with(LogicalRect::default(), |_| Unsupported);
/// }
/// ```
pub trait Command: CustomCommand {
    type Renderer: Default;
}

/// renders one command type with persistent state owned by the handler
pub trait Extension<P: Pixel>: 'static {
    type Command: CustomCommand;
    type Prepared: 'static;

    fn prepare(
        &mut self,
        command: <Self::Command as CustomCommand>::View<'_>,
        cx: &mut PrepareContext<'_>,
    ) -> Self::Prepared;

    /// without source reads callbacks may cover individual spans in any order
    fn render(&mut self, prepared: &Self::Prepared, cx: &mut DrawContext<'_, P>);

    /// returns the source pixels needed for an output region in physical pixels
    /// returning a footprint enables source reads and propagates damage
    fn read_bounds(
        &self,
        _command: <Self::Command as CustomCommand>::View<'_>,
        _output: PhysicalRect,
        _scale: f32,
    ) -> Option<PhysicalRect> {
        None
    }

    fn finish_frame(&mut self) {}
}

pub struct PrepareContext<'a> {
    pub scale: f32,
    pub bounds: PhysicalRect,
    pub data: Writer<'a>,
    pub(crate) images: &'a mut ImageArena<StoredImage>,
}

pub struct Span<'a, P> {
    pub x: i32,
    pub y: i32,
    pub pixels: &'a mut [P],
    /// clip coverage to apply when blending pixels
    pub coverage: u8,
    pub data: &'a Arena,
    pub(crate) images: &'a ImageArena<StoredImage>,
}

pub struct DrawContext<'a, P> {
    source: Option<&'a Surface<P>>,
    target: DrawTarget<'a, P>,
}

pub struct Surface<P> {
    width: usize,
    height: usize,
    pixels: Vec<P>,
    valid: Region,
}

impl PrepareContext<'_> {
    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        data.validate();
        let id = self
            .images
            .insert_with(|id| StoredImage::new(ImageHandle::new(id.into(), data.size), data));
        self.images.get(id).unwrap().handle.clone()
    }
}

impl<'a, P: Pixel> DrawContext<'a, P> {
    pub fn source(&self) -> Option<&'a Surface<P>> {
        self.source
    }

    pub fn output_regions(&self) -> impl Iterator<Item = PhysicalRect> + '_ {
        let (span, pass) = match &self.target {
            DrawTarget::Span(span) => (
                Some(PhysicalRect::new(span.x, span.y, span.pixels.len() as i32, 1)),
                None,
            ),
            DrawTarget::Pass { output, bounds, .. } => (None, Some((*output, *bounds))),
        };
        span.into_iter().chain(
            pass.into_iter()
                .flat_map(|(output, bounds)| output.rects().filter_map(move |region| region.intersection(bounds))),
        )
    }

    pub fn for_each_span(&mut self, mut draw: impl FnMut(Span<'_, P>)) {
        match &mut self.target {
            DrawTarget::Span(span) => draw(Span {
                x: span.x,
                y: span.y,
                pixels: span.pixels,
                coverage: span.coverage,
                data: span.data,
                images: span.images,
            }),
            DrawTarget::Pass {
                target,
                clips,
                bounds,
                clip,
                data,
                images,
                output,
            } => {
                for bounds in output.rects().filter_map(|region| region.intersection(*bounds)) {
                    for y in bounds.y..bounds.y + bounds.height {
                        clips.for_each(*clip, y, bounds.x..bounds.x + bounds.width, |range, coverage| {
                            target.process_line(y as usize, range.start as usize..range.end as usize, &mut |pixels| {
                                draw(Span {
                                    x: range.start,
                                    y,
                                    coverage,
                                    data,
                                    images,
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

impl<'a, P> Span<'a, P> {
    pub fn image(&self, image: &ImageHandle) -> Option<&'a ImageData> {
        self.images.get(image.id().into()).map(|image| &image.data)
    }
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

pub(crate) mod registry;

enum DrawTarget<'a, P> {
    Span(Span<'a, P>),
    Pass {
        target: &'a mut dyn Target<P>,
        clips: &'a ClipStack,
        bounds: PhysicalRect,
        clip: u32,
        data: &'a Arena,
        images: &'a ImageArena<StoredImage>,
        output: &'a Region,
    },
}

trait Target<P> {
    fn process_line(&mut self, y: usize, range: std::ops::Range<usize>, draw: &mut dyn FnMut(&mut [P]));
}

impl<B: PixelBuffer> Target<B::Pixel> for B {
    fn process_line(&mut self, y: usize, range: std::ops::Range<usize>, draw: &mut dyn FnMut(&mut [B::Pixel])) {
        PixelBuffer::process_line(self, y, range, |pixels| draw(pixels));
    }
}
