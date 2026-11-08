use std::{
    any::TypeId,
    hash::{Hash, Hasher},
    marker::PhantomData,
    ptr::NonNull,
};

use blit::{LogicalRect, Paint, PhysicalRect, Scale2, internal::hash::FxHasher};
use blit_gui::{
    TextSystem,
    image::{ImageData, ImageHandle, ImageRequest},
    paint::{Mesh, Rectangle},
    style::Fill,
    text::TextRequest,
};

use crate::{
    DamageTracker, DrawContext, Pixel, PixelBuffer, RenderStrategy, Renderer, Span, StoredImage, Surface,
    arena::{Arena, Value},
    damage::{DamageInput, Record, region::Region},
    strategy::{clip::ClipStack, command::CommandList},
    text::TextRenderer,
};

/// consumes drawing data and records cpu work before its borrows expire
///
/// ```
/// use blit::{LogicalRect, Paint};
/// use blit_cpu::{DamageTracker, Handler, PaintContext, Pixel, PixelBuffer, Renderer, RenderStrategy};
/// use blit_gui::{color::Color, paint::Rectangle};
///
/// struct Swatch(Color);
/// #[derive(Default)]
/// struct Painter;
///
/// impl<P: Pixel> Handler<Swatch, P> for Painter {
///     fn handle(&mut self, swatch: Swatch, cx: &mut PaintContext<'_, Self, P>) {
///         let area = cx.bounds.to_logical(blit::Scale2::uniform(cx.scale));
///         cx.rectangle(Rectangle::new(area).background(swatch.0));
///     }
/// }
///
/// impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<Swatch> for Renderer<B, S, D> {
///     fn paint(&mut self, bounds: LogicalRect, swatch: Swatch) {
///         self.handle::<Painter, _>(bounds, swatch);
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

pub struct PaintContext<'a, H, P: Pixel> {
    pub scale: f32,
    pub bounds: PhysicalRect,
    clip: u32,
    handler: NonNull<()>,
    marker: PhantomData<fn(H) -> H>,
    damage_input: &'a mut DamageInput,
    commands: &'a mut CommandList<P>,
    pub(crate) images: &'a mut blit_arena::Arena<StoredImage>,
    text: &'a mut TextRenderer,
    text_system: &'a mut TextSystem,
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Renderer<B, S, D> {
    /// configures one handler before recording begins
    pub fn handler<H: 'static>(mut self, handler: H) -> Self {
        assert!(
            self.damage_input.records.is_empty(),
            "cannot replace a handler during a frame"
        );
        let kind = TypeId::of::<H>();
        if let Some(value) = self.handlers.iter_mut().find(|value| value.kind == kind) {
            *value = ErasedHandler::new(handler);
        } else {
            self.handlers.push(ErasedHandler::new(handler));
        }
        self.damage.invalidate();
        self
    }

    /// returns a handler and invalidates retained pixels
    pub fn handler_mut<H: Default + 'static>(&mut self) -> &mut H {
        assert!(
            self.damage_input.records.is_empty(),
            "cannot configure a handler during a frame"
        );
        self.damage.invalidate();
        let index = self.handler_index::<H>();
        // safety: the lookup selects the entry created for H
        unsafe { self.handlers[index].value.cast::<H>().as_mut() }
    }

    /// dispatches drawing data to its handler and creates its default on first use
    pub fn handle<H: Handler<T, B::Pixel> + Default, T>(&mut self, bounds: LogicalRect, command: T) {
        let index = self.handler_index::<H>();
        let handler = self.handlers[index].value;
        let context = &mut self.context;
        // safety: the lookup selects the entry created for H
        unsafe { handler.cast::<H>().as_mut() }.handle(
            command,
            &mut PaintContext {
                scale: context.scale_factor,
                bounds: bounds.to_physical(Scale2::uniform(context.scale_factor)),
                clip: self.paint_clip,
                handler,
                marker: PhantomData,
                damage_input: &mut self.damage_input,
                commands: &mut context.commands,
                images: &mut context.images,
                text: &mut context.text,
                text_system: &mut self.text_system,
            },
        );
    }

    fn handler_index<H: Default + 'static>(&mut self) -> usize {
        let kind = TypeId::of::<H>();
        if let Some(index) = self.handlers.iter().position(|value| value.kind == kind) {
            index
        } else {
            let index = self.handlers.len();
            self.handlers.push(ErasedHandler::new(H::default()));
            index
        }
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<Rectangle<'_>> for Renderer<B, S, D> {
    fn paint(&mut self, bounds: LogicalRect, command: Rectangle<'_>) {
        self.handle::<(), _>(bounds, command);
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<ImageRequest> for Renderer<B, S, D> {
    fn paint(&mut self, bounds: LogicalRect, command: ImageRequest) {
        self.handle::<(), _>(bounds, command);
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<TextRequest<'_>> for Renderer<B, S, D> {
    fn paint(&mut self, bounds: LogicalRect, command: TextRequest<'_>) {
        self.handle::<(), _>(bounds, command);
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<Mesh<'_>> for Renderer<B, S, D> {
    fn paint(&mut self, bounds: LogicalRect, command: Mesh<'_>) {
        self.handle::<(), _>(bounds, command);
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<blit_gui::atom::Clear> for Renderer<B, S, D> {
    fn paint(&mut self, bounds: LogicalRect, command: blit_gui::atom::Clear) {
        self.handle::<(), _>(bounds, command);
    }
}

impl<P: Pixel> Handler<Rectangle<'_>, P> for () {
    fn handle(&mut self, command: Rectangle<'_>, cx: &mut PaintContext<'_, Self, P>) {
        cx.rectangle(command);
    }
}

impl<P: Pixel> Handler<ImageRequest, P> for () {
    fn handle(&mut self, command: ImageRequest, cx: &mut PaintContext<'_, Self, P>) {
        cx.image(command);
    }
}

impl<P: Pixel> Handler<TextRequest<'_>, P> for () {
    fn handle(&mut self, command: TextRequest<'_>, cx: &mut PaintContext<'_, Self, P>) {
        cx.text(command);
    }
}

impl<P: Pixel> Handler<Mesh<'_>, P> for () {
    fn handle(&mut self, command: Mesh<'_>, cx: &mut PaintContext<'_, Self, P>) {
        cx.mesh(command);
    }
}

impl<P: Pixel> Handler<blit_gui::atom::Clear, P> for () {
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
        self.record_draw(key, value, draw, None);
    }

    /// prepares once on the first visible span and reuses the result for the frame
    pub fn draw_prepared<T: 'static, R: 'static>(
        &mut self,
        key: &impl Hash,
        value: T,
        prepare: fn(&mut H, &T, &mut DrawContext<'_, P>) -> R,
        draw: fn(&mut H, &R, &mut DrawContext<'_, P>),
    ) {
        struct Input<H, T, R, P> {
            value: T,
            prepare: fn(&mut H, &T, &mut DrawContext<'_, P>) -> R,
            draw: fn(&mut H, &R, &mut DrawContext<'_, P>),
        }
        let call = Callback {
            handler: self.handler,
            value: self.commands.data.push(Input { value, prepare, draw }).erase(),
            prepare: Some(|handler, value, data, prepared, cx| {
                let input = data.value::<Input<H, T, R, P>>(value).unwrap();
                // safety: the context records the pointer selected for H
                let value = (input.prepare)(unsafe { handler.cast::<H>().as_mut() }, &input.value, cx);
                prepared
                    .push(Call {
                        value,
                        draw: input.draw,
                        reads: None,
                    })
                    .erase()
            }),
            draw: Call::<H, R, P>::draw,
            reads: None,
        };
        self.record::<T>(key, call);
    }

    /// records a framebuffer effect with its source footprint for each output region
    pub fn pass<T: 'static>(
        &mut self,
        key: &impl Hash,
        value: T,
        draw: fn(&mut H, &T, &mut DrawContext<'_, P>),
        reads: fn(&H, &T, PhysicalRect) -> PhysicalRect,
    ) {
        self.record_draw(key, value, draw, Some(reads));
    }

    fn record_draw<T: 'static>(
        &mut self,
        key: &impl Hash,
        value: T,
        draw: fn(&mut H, &T, &mut DrawContext<'_, P>),
        reads: Option<fn(&H, &T, PhysicalRect) -> PhysicalRect>,
    ) {
        let call = Callback {
            handler: self.handler,
            value: self.commands.prepared.push(Call { value, draw, reads }).erase(),
            prepare: None,
            draw: Call::<H, T, P>::draw,
            reads: reads.map(|_| {
                (|handler: NonNull<()>, value, data: &Arena, output| {
                    let call = data.value::<Call<H, T, P>>(value).unwrap();
                    // safety: the context records the pointer selected for H
                    call.reads.unwrap()(unsafe { handler.cast::<H>().as_ref() }, &call.value, output)
                }) as _
            }),
        };
        self.record::<T>(key, call);
    }

    fn record<T: 'static>(&mut self, key: &impl Hash, call: Callback<P>) {
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

pub struct Callback<P> {
    handler: NonNull<()>,
    value: Value,
    prepare: Option<unsafe fn(NonNull<()>, Value, &Arena, &mut Arena, &mut DrawContext<'_, P>) -> Value>,
    draw: unsafe fn(NonNull<()>, Value, &Arena, &mut DrawContext<'_, P>),
    pub reads: Option<unsafe fn(NonNull<()>, Value, &Arena, PhysicalRect) -> PhysicalRect>,
}

impl<P> Callback<P> {
    pub fn draw(&mut self, data: &Arena, prepared: &mut Arena, cx: &mut DrawContext<'_, P>) {
        if let Some(prepare) = self.prepare {
            // safety: recording pairs the handler pointer with its typed callback
            let value = unsafe { prepare(self.handler, self.value, data, prepared, cx) };
            self.value = value;
            self.prepare = None;
        }
        // safety: recording pairs the handler pointer with its typed callback
        // handlers cannot be replaced while commands are pending
        unsafe { (self.draw)(self.handler, self.value, prepared, cx) };
    }

    pub fn reads(&self, data: &Arena, output: PhysicalRect) -> PhysicalRect {
        // safety: recording pairs the handler pointer with its typed callback
        // handlers cannot be replaced while commands are pending
        unsafe { (self.reads.unwrap())(self.handler, self.value, data, output) }
    }
}

struct Call<H, T, P> {
    value: T,
    draw: fn(&mut H, &T, &mut DrawContext<'_, P>),
    reads: Option<fn(&H, &T, PhysicalRect) -> PhysicalRect>,
}

impl<H: 'static, T: 'static, P: Pixel> Call<H, T, P> {
    unsafe fn draw(handler: NonNull<()>, value: Value, data: &Arena, cx: &mut DrawContext<'_, P>) {
        let call = data.value::<Self>(value).unwrap();
        // safety: recording pairs the handler pointer with this typed callback
        (call.draw)(unsafe { handler.cast::<H>().as_mut() }, &call.value, cx);
    }
}

pub struct ErasedHandler {
    kind: TypeId,
    value: NonNull<()>,
    drop: unsafe fn(NonNull<()>),
}

impl ErasedHandler {
    fn new<H: 'static>(handler: H) -> Self {
        Self {
            kind: TypeId::of::<H>(),
            value: NonNull::from(Box::leak(Box::new(handler))).cast(),
            drop: |value| {
                // safety: new transfers ownership of one boxed H to this entry
                unsafe { drop(Box::from_raw(value.cast::<H>().as_ptr())) };
            },
        }
    }
}

impl Drop for ErasedHandler {
    fn drop(&mut self) {
        // safety: new pairs the allocation with its destructor and entries are never copied
        unsafe { (self.drop)(self.value) };
    }
}

impl<'a, P: Pixel> DrawContext<'a, P> {
    pub fn data(&self) -> &Arena {
        match &self.target {
            DrawTarget::Span { data, .. } | DrawTarget::Pass { data, .. } => data,
        }
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        data.validate();
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
            DrawTarget::Span {
                x,
                y,
                pixels,
                coverage,
                data,
            } => draw(Span {
                x: *x,
                y: *y,
                pixels,
                coverage: *coverage,
                data,
                images: self.images,
            }),
            DrawTarget::Pass {
                target,
                clips,
                clip,
                data,
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
                                    data,
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
    Span {
        x: i32,
        y: i32,
        pixels: &'a mut [P],
        coverage: u8,
        data: &'a Arena,
    },
    Pass {
        target: &'a mut dyn Target<P>,
        clips: &'a ClipStack,
        clip: u32,
        data: &'a Arena,
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
