use std::{
    any::TypeId,
    hash::{Hash, Hasher},
};

use blit::{PhysicalRect, Scale2, internal::hash::FxHasher};
use blit_arena::Arena as ImageArena;
use blit_gui::{
    image::{ImageData, ImageHandle, ImageRequest},
    paint::{Mesh, Rectangle},
    style::Fill,
    text::TextRequest,
};

use crate::{
    DrawContext, PaintContext, Pixel, PixelBuffer, Span, StoredImage, Surface,
    arena::{Arena, Value},
    damage::{Record, region::Region},
    strategy::clip::ClipStack,
};

/// consumes drawing data and records cpu work before its borrows expire
///
/// ```
/// use blit_cpu::{Handler, PaintContext, Xrgb8888};
/// use blit_gui::{color::Color, paint::Rectangle};
///
/// struct Swatch(Color);
/// struct Painter;
///
/// impl Handler<Swatch, Xrgb8888> for Painter {
///     fn handle(&mut self, swatch: Swatch, cx: &mut PaintContext<'_, Self, Xrgb8888>) {
///         let area = cx.bounds.to_logical(blit::Scale2::uniform(cx.scale));
///         cx.rectangle(Rectangle::new(area).background(swatch.0));
///     }
/// }
/// ```
///
/// ```compile_fail
/// use blit::{LogicalRect, Paint};
/// use blit_cpu::{Renderer, VecBuffer, Xrgb8888};
/// struct Unsupported;
/// fn draw(renderer: &mut Renderer<VecBuffer<Xrgb8888>>) {
///     renderer.paint(LogicalRect::default(), Unsupported);
/// }
/// ```
pub trait Handler<T, P: Pixel>: Sized + 'static {
    fn handle(&mut self, command: T, cx: &mut PaintContext<'_, Self, P>);
}

impl<P: Pixel, H: 'static> Handler<Rectangle<'_>, P> for H {
    fn handle(&mut self, command: Rectangle<'_>, cx: &mut PaintContext<'_, Self, P>) {
        cx.rectangle(command);
    }
}

impl<P: Pixel, H: 'static> Handler<ImageRequest, P> for H {
    fn handle(&mut self, command: ImageRequest, cx: &mut PaintContext<'_, Self, P>) {
        cx.image(command);
    }
}

impl<P: Pixel, H: 'static> Handler<TextRequest<'_>, P> for H {
    fn handle(&mut self, command: TextRequest<'_>, cx: &mut PaintContext<'_, Self, P>) {
        cx.text(command);
    }
}

impl<P: Pixel, H: 'static> Handler<Mesh<'_>, P> for H {
    fn handle(&mut self, command: Mesh<'_>, cx: &mut PaintContext<'_, Self, P>) {
        cx.mesh(command);
    }
}

impl<P: Pixel, H: 'static> Handler<blit_gui::atom::Clear, P> for H {
    fn handle(&mut self, _: blit_gui::atom::Clear, cx: &mut PaintContext<'_, Self, P>) {
        cx.clear();
    }
}

impl<H: 'static, P: Pixel> PaintContext<'_, H, P> {
    pub fn data(&mut self) -> crate::arena::Writer<'_> {
        self.commands.data.writer()
    }

    /// records a raster callback with a key describing everything it draws
    pub fn draw<T: 'static>(&mut self, key: &impl Hash, value: T, draw: fn(&mut H, &T, &mut DrawContext<'_, P>)) {
        self.record(key, value, draw, None);
    }

    /// records a framebuffer effect with its source footprint for each output region
    pub fn pass<T: 'static>(
        &mut self,
        key: &impl Hash,
        value: T,
        draw: fn(&mut H, &T, &mut DrawContext<'_, P>),
        reads: fn(&H, &T, PhysicalRect) -> PhysicalRect,
    ) {
        self.record(key, value, draw, Some(reads));
    }

    fn record<T: 'static>(
        &mut self,
        key: &impl Hash,
        value: T,
        draw: fn(&mut H, &T, &mut DrawContext<'_, P>),
        reads: Option<fn(&H, &T, PhysicalRect) -> PhysicalRect>,
    ) {
        struct Call<H, T, P> {
            value: T,
            draw: fn(&mut H, &T, &mut DrawContext<'_, P>),
            reads: Option<fn(&H, &T, PhysicalRect) -> PhysicalRect>,
        }
        let mut hash = FxHasher::default();
        TypeId::of::<H>().hash(&mut hash);
        TypeId::of::<T>().hash(&mut hash);
        std::any::type_name_of_val(key).hash(&mut hash);
        key.hash(&mut hash);
        self.damage_input.records.push(Record {
            bounds: self.bounds,
            clip: self.clip,
            fingerprint: hash.finish(),
        });
        let call = Callback {
            value: self.commands.data.push(Call { value, draw, reads }).erase(),
            draw: |handlers, value, data, cx| {
                let call = data.value::<Call<H, T, P>>(value).unwrap();
                (call.draw)(handlers, &call.value, cx);
            },
            reads: reads.map(|_| {
                (|handlers: &H, value, data: &Arena, output| {
                    let call = data.value::<Call<H, T, P>>(value).unwrap();
                    call.reads.unwrap()(handlers, &call.value, output)
                }) as _
            }),
        };
        self.commands.push_callback(call, self.bounds, self.clip);
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        data.validate();
        let id = self
            .images
            .insert_with(|id| StoredImage::new(ImageHandle::new(id.into(), data.size), data));
        self.images.get(id).unwrap().handle.clone()
    }

    pub fn rectangle(&mut self, rectangle: Rectangle<'_>) {
        let mut hash = FxHasher::default();
        hash.write_u8(0);
        [
            rectangle.area.x,
            rectangle.area.y,
            rectangle.area.width,
            rectangle.area.height,
            rectangle.radius.top_left,
            rectangle.radius.top_right,
            rectangle.radius.bottom_right,
            rectangle.radius.bottom_left,
            rectangle.opacity,
            rectangle.border.width,
        ]
        .map(f32::to_bits)
        .hash(&mut hash);
        for fill in [rectangle.background, rectangle.border.fill] {
            std::mem::discriminant(&fill).hash(&mut hash);
            match fill {
                Fill::Solid(color) => color.hash(&mut hash),
                Fill::LinearGradient(gradient) => {
                    assert!(
                        gradient.stops.len() >= 2
                            && gradient.angle_degrees.is_finite()
                            && gradient
                                .stops
                                .iter()
                                .all(|stop| stop.position.is_finite() && (0.0..=1.0).contains(&stop.position))
                            && gradient
                                .stops
                                .windows(2)
                                .all(|stops| stops[0].position < stops[1].position),
                        "invalid linear gradient"
                    );
                    hash.write_u32(gradient.angle_degrees.to_bits());
                    gradient.stops.len().hash(&mut hash);
                    for stop in gradient.stops {
                        hash.write_u32(stop.position.to_bits());
                        stop.color.hash(&mut hash);
                    }
                }
            }
        }
        self.damage_input.records.push(Record {
            bounds: self.bounds,
            clip: self.clip,
            fingerprint: hash.finish(),
        });
        if let Some(rectangle) =
            crate::render::rectangle::Prepared::new(&rectangle, self.scale, &mut self.commands.gradients)
            && let Some(bounds) = rectangle.geometry.intersection(self.bounds)
        {
            self.commands.push_rectangle(rectangle, bounds, self.clip);
        }
    }

    pub fn image(&mut self, request: ImageRequest) {
        let mut hash = FxHasher::default();
        hash.write_u8(1);
        request.image.hash(&mut hash);
        [
            request.area.x,
            request.area.y,
            request.area.width,
            request.area.height,
            request.opacity,
        ]
        .map(f32::to_bits)
        .hash(&mut hash);
        std::mem::discriminant(&request.fit).hash(&mut hash);
        std::mem::discriminant(&request.sampling).hash(&mut hash);
        std::mem::discriminant(&request.horizontal_tiling).hash(&mut hash);
        std::mem::discriminant(&request.vertical_tiling).hash(&mut hash);
        request.colorize.hash(&mut hash);
        request
            .nine_slice
            .map(|slice| [slice.top, slice.right, slice.bottom, slice.left])
            .hash(&mut hash);
        self.damage_input.records.push(Record {
            bounds: self.bounds,
            clip: self.clip,
            fingerprint: hash.finish(),
        });
        if let Some(texture) = self.images.get(request.image.into()) {
            crate::render::image::prepare(&request, &texture.data, self.bounds, self.scale, |image, bounds| {
                self.commands.push_image(
                    image,
                    bounds,
                    self.clip,
                    image.is_opaque(&texture.data, texture.opaque),
                    image.has_opaque_spans(&texture.data, texture.has_opaque_spans),
                );
            });
        }
    }

    pub fn text(&mut self, request: TextRequest<'_>) {
        let mut hash = FxHasher::default();
        hash.write_u8(2);
        request.text.hash(&mut hash);
        request.colors.hash(&mut hash);
        [
            request.area.x,
            request.area.y,
            request.area.width,
            request.area.height,
            request.offset_x,
        ]
        .map(f32::to_bits)
        .hash(&mut hash);
        request.options.wrap.hash(&mut hash);
        request.options.overflow.hash(&mut hash);
        request.options.horizontal_align.hash(&mut hash);
        request.options.vertical_align.hash(&mut hash);
        request.options.max_lines.hash(&mut hash);
        self.damage_input.records.push(Record {
            bounds: self.bounds,
            clip: self.clip,
            fingerprint: hash.finish(),
        });
        let area = request.area.to_physical(Scale2::uniform(self.scale));
        if let Some(visible) = area.intersection(self.bounds) {
            let (prepared, paragraph_bounds) = self.text.prepare(self.text_system, &request, self.scale);
            if let Some(bounds) = paragraph_bounds.intersection(visible) {
                self.commands.push_text(prepared, bounds, self.clip);
            }
        }
    }

    pub fn mesh(&mut self, mesh: Mesh<'_>) {
        assert_eq!(mesh.indices.len() % 3, 0, "mesh index count must be divisible by three");
        let mut hash = FxHasher::default();
        hash.write_u8(3);
        mesh.vertices.len().hash(&mut hash);
        for vertex in mesh.vertices {
            assert!(
                vertex.position.x.is_finite() && vertex.position.y.is_finite(),
                "mesh positions must be finite"
            );
            [vertex.position.x, vertex.position.y].map(f32::to_bits).hash(&mut hash);
            vertex.color.hash(&mut hash);
        }
        mesh.indices.hash(&mut hash);
        self.damage_input.records.push(Record {
            bounds: self.bounds,
            clip: self.clip,
            fingerprint: hash.finish(),
        });
        for indices in mesh.indices.as_chunks::<3>().0 {
            let vertices = indices.map(|index| mesh.vertices[index as usize]);
            if let Some((triangle, bounds)) =
                crate::render::triangle::Prepared::new(vertices, Scale2::uniform(self.scale), self.bounds)
            {
                self.commands.push_triangle(triangle, bounds, self.clip);
            }
        }
    }

    pub fn clear(&mut self) {
        self.damage_input.records.push(Record {
            bounds: self.bounds,
            clip: 0,
            fingerprint: u64::MAX,
        });
        self.commands.push_clear(self.bounds);
    }
}

pub struct Callback<H, P> {
    pub value: Value,
    pub draw: fn(&mut H, Value, &Arena, &mut DrawContext<'_, P>),
    pub reads: Option<fn(&H, Value, &Arena, PhysicalRect) -> PhysicalRect>,
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

pub enum DrawTarget<'a, P> {
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

pub trait Target<P> {
    fn process_line(&mut self, y: usize, range: std::ops::Range<usize>, draw: &mut dyn FnMut(&mut [P]));
}

impl<B: PixelBuffer> Target<B::Pixel> for B {
    fn process_line(&mut self, y: usize, range: std::ops::Range<usize>, draw: &mut dyn FnMut(&mut [B::Pixel])) {
        PixelBuffer::process_line(self, y, range, |pixels| draw(pixels));
    }
}
