use std::{any::TypeId, hash::Hash, marker::PhantomData, mem::offset_of, ptr::NonNull};

use blit::{
    PhysicalRect, Scale2,
    internal::{
        bump::{Bump, Offset},
        hash::FxHasher,
    },
};
use blit_gui::{
    display_list::{Command, DisplayList, Ops, Slice},
    image::{ImageData, ImageHandle, prepare_image_patches},
    paint::{ImageRequest, Mesh, MeshVertex, Rectangle, TextRequest},
    style::{Border, Fill, GradientStop, LinearGradient},
};

use crate::{
    DrawContext, Pixel, PremultipliedRgbaColor, StoredImage,
    strategy::command::{Coverage, Payload, RasterCommands},
};

/// consumes drawing data and records cpu work before its borrows expire
///
/// ```
/// use blit::{LogicalRect, Paint};
/// use blit_gui::damage::DamageTracker;
/// use blit_cpu::{Handler, PaintContext, Pixel, PixelBuffer, Renderer, RenderStrategy};
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

    /// called after each frame once the handler has been used
    fn finish_frame(&mut self) {}
}

pub struct PaintContext<'a, H, P: Pixel> {
    pub scale: f32,
    pub bounds: PhysicalRect,
    pub(crate) clip: u32,
    pub(crate) handler: NonNull<()>,
    pub(crate) marker: PhantomData<fn(H, P) -> (H, P)>,
    pub(crate) list: &'a mut DisplayList,
    pub(crate) images: &'a mut blit_arena::Arena<StoredImage>,
}

impl<H: 'static, P: Pixel> PaintContext<'_, H, P> {
    /// prepares an image with an explicit logical to physical scale
    pub fn prepare_image(&self, request: &ImageRequest, scale: f32) -> Option<crate::raster::Image> {
        crate::raster::Image::new(request, scale, self.bounds, self.images)
    }

    pub fn store_slice<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> Slice<T> {
        self.list.store_slice(values)
    }

    /// records a raster callback with an owned snapshot of its visual inputs
    /// opaque guarantees fully opaque output inside a physical rectangle intersected with bounds and clip
    pub fn draw<T: 'static>(
        &mut self,
        key: impl PartialEq + Hash + 'static,
        value: T,
        opaque: Option<PhysicalRect>,
        draw: fn(&mut H, &T, &mut DrawContext<'_, P>),
    ) {
        self.record_draw(key, value, opaque, draw, None);
    }

    /// prepares once on the first visible span and reuses the result for the frame
    /// opaque guarantees fully opaque output inside a physical rectangle intersected with bounds and clip
    pub fn draw_prepared<T: 'static, R: 'static>(
        &mut self,
        key: impl PartialEq + Hash + 'static,
        value: T,
        opaque: Option<PhysicalRect>,
        prepare: fn(&mut H, &T, &mut DrawContext<'_, P>) -> R,
        draw: fn(&mut H, &R, &mut DrawContext<'_, P>),
    ) {
        let handler = self.handler;
        self.record(
            self.bounds,
            self.clip,
            key,
            value,
            opaque.unwrap_or_default(),
            move |_, offset, command, _, _, _, target| {
                let call = ErasedCommand {
                    draw: Some(Call::<H, R, P>::draw),
                    ..ErasedCommand::prepared(
                        &mut target.data,
                        handler,
                        (offset, prepare, draw),
                        |handler, (offset, prepare, draw), cx| {
                            // safety: recording pairs this offset with T
                            let value = unsafe { cx.list.data().get(Offset::<T>::new(*offset)) };
                            Call {
                                value: prepare(handler, value, cx),
                                draw: *draw,
                                reads: None,
                            }
                        },
                    )
                };
                target.push(call, command.bounds, command.clip);
            },
        );
    }

    /// records a framebuffer effect with its source footprint for each output region
    pub fn pass<T: 'static>(
        &mut self,
        key: impl PartialEq + Hash + 'static,
        value: T,
        draw: fn(&mut H, &T, &mut DrawContext<'_, P>),
        reads: fn(&H, &T, PhysicalRect) -> PhysicalRect,
    ) {
        self.record_draw(key, value, None, draw, Some(reads));
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        data.validate();
        let id = self
            .images
            .insert_with(|id| StoredImage::new(ImageHandle::new(id.into(), data.size), data));
        self.images.get(id).unwrap().handle.clone()
    }

    pub fn rectangle(&mut self, rectangle: Rectangle<'_>) {
        let mut stops = [None; 2];
        let fills = [rectangle.background, rectangle.border.fill];
        let fills = [0, 1].map(|index| match fills[index] {
            Fill::Solid(color) => Fill::Solid(color),
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
                stops[index] = Some(self.list.store_slice(gradient.stops.iter().copied()));
                Fill::LinearGradient(LinearGradient { stops: &[], ..gradient })
            }
        });
        let value = StoredRectangle {
            rectangle: Rectangle {
                background: fills[0],
                border: Border {
                    fill: fills[1],
                    ..rectangle.border
                },
                ..rectangle
            },
            stops,
        };
        self.record_input(
            self.bounds,
            self.clip,
            value,
            |a, left, b, right| a.view(left) == b.view(right),
            |value, list, state| value.view(list).hash(state),
            |input, offset, record, _, context| {
                let scale = context.scale;
                let target = &mut *context.commands;
                let rectangle = input.rectangle;
                let Some(bounds) = rectangle
                    .area
                    .to_physical(Scale2::uniform(scale))
                    .intersection(record.bounds)
                else {
                    return;
                };
                if rectangle.opacity <= 0.0 || rectangle.opacity.is_nan() {
                    return;
                }
                let coverage = match rectangle.background {
                    Fill::Solid(color)
                        if PremultipliedRgbaColor::with_opacity(color, rectangle.opacity).alpha == 255 =>
                    {
                        if rectangle.radius == Default::default() {
                            Coverage::Full
                        } else {
                            Coverage::Rounded
                        }
                    }
                    _ => Coverage::None,
                };
                if record.clip == 0
                    && coverage == Coverage::Full
                    && let Some(previous) = target.metadata.last()
                    && previous.clip == 0
                    && previous.bounds == bounds
                {
                    target.metadata.pop();
                    target.entries.pop();
                    let removed = target.len();
                    for indices in [&mut target.meshes, &mut target.passes] {
                        if indices.last() == Some(&removed) {
                            indices.pop();
                        }
                    }
                }
                target
                    .record(bounds, record.clip, (scale, offset), |_, (scale, value), cx| {
                        // safety: recording stores this offset as StoredRectangle
                        let input = unsafe { cx.list.data().get(Offset::<StoredRectangle>::new(*value)) };
                        crate::render::rectangle::Prepared::new(&input.view(cx.list), *scale, cx.gradients)
                            .map(Payload::Rectangle)
                            .unwrap_or(Payload::Empty)
                    })
                    .coverage = coverage;
            },
        );
    }

    pub fn image(&mut self, request: ImageRequest) {
        self.record_input(
            self.bounds,
            self.clip,
            request,
            |a, _, b, _| a == b,
            |value, _, state| value.hash(state),
            |request, offset, record, _, context| {
                let scale = context.scale;
                let target = &mut *context.commands;
                if let Some(texture) = context.images.get(request.image.into()) {
                    prepare_image_patches(request, texture.data.size, scale, |patch| {
                        if let Some(bounds) = patch.bounds.intersection(record.bounds) {
                            let (opaque, partial) = crate::render::image::coverage(
                                request,
                                patch,
                                &texture.data,
                                texture.opaque,
                                texture.has_opaque_spans,
                            );
                            target
                                .record(
                                    bounds,
                                    record.clip,
                                    (scale, offset, patch),
                                    |_, (scale, value, patch), cx| {
                                        // safety: recording stores this offset as ImageRequest
                                        let request =
                                            unsafe { cx.list.data().get(Offset::<ImageRequest>::new(*value)) };
                                        crate::render::image_patch::Prepared::new(request, *patch, *scale)
                                            .map(Payload::Image)
                                            .unwrap_or(Payload::Empty)
                                    },
                                )
                                .coverage = if opaque {
                                Coverage::Full
                            } else if partial {
                                Coverage::PartialImage
                            } else {
                                Coverage::None
                            };
                        }
                    });
                }
            },
        );
    }

    pub fn text(&mut self, request: TextRequest<'_>) {
        let colors = self.list.store_slice(request.colors.iter().copied());
        let value = StoredText {
            request: TextRequest { colors: &[], ..request },
            colors,
        };
        self.record_input(
            self.bounds,
            self.clip,
            value,
            |a, left, b, right| a.view(left) == b.view(right),
            |value, list, state| value.view(list).hash(state),
            |input, offset, record, _, context| {
                let scale = context.scale;
                let target = &mut *context.commands;
                let area = input.request.area.to_physical(Scale2::uniform(scale));
                if let Some(bounds) = area.intersection(record.bounds) {
                    target.record(bounds, record.clip, (scale, offset), |_, (scale, value), cx| {
                        // safety: recording stores this offset as StoredText
                        let input = unsafe { cx.list.data().get(Offset::<StoredText>::new(*value)) };
                        let (text, bounds) = cx.text.prepare(&input.view(cx.list), *scale);
                        Payload::Text(text, bounds)
                    });
                }
            },
        );
    }

    pub fn mesh(&mut self, mesh: Mesh<'_>) {
        assert_eq!(mesh.indices.len() % 3, 0, "mesh index count must be divisible by three");
        if mesh.indices.is_empty() {
            return;
        }
        assert!(
            mesh.indices.iter().all(|index| (*index as usize) < mesh.vertices.len()),
            "mesh index out of bounds"
        );
        assert!(
            mesh.vertices
                .iter()
                .all(|vertex| vertex.position.x.is_finite() && vertex.position.y.is_finite()),
            "mesh positions must be finite"
        );
        let vertices = self.list.store_slice(mesh.vertices.iter().copied());
        let indices = self.list.store_slice(mesh.indices.iter().copied());
        let value = StoredMesh { vertices, indices };
        self.record_input(
            self.bounds,
            self.clip,
            value,
            |a, left, b, right| a.view(left) == b.view(right),
            |value, list, state| value.view(list).hash(state),
            |input, _, record, _, context| {
                let scale = context.scale;
                let target = &mut *context.commands;
                let call = ErasedCommand::ready(
                    NonNull::<()>::dangling(),
                    &mut target.prepared,
                    Payload::Mesh(input.vertices, input.indices, scale),
                );
                let index = target.len();
                target.push(call, record.bounds, record.clip);
                target.meshes.push(index);
            },
        );
    }

    pub fn clear(&mut self) {
        self.record(
            self.bounds,
            0,
            (),
            (),
            PhysicalRect::default(),
            |_, _, record, _, _, _, target| {
                let call = ErasedCommand::ready(NonNull::<()>::dangling(), &mut target.prepared, Payload::Clear);
                target.push(call, record.bounds, 0).coverage = Coverage::Full;
            },
        );
    }

    fn record_draw<T: 'static>(
        &mut self,
        key: impl PartialEq + Hash + 'static,
        value: T,
        opaque: Option<PhysicalRect>,
        draw: fn(&mut H, &T, &mut DrawContext<'_, P>),
        reads: Option<fn(&H, &T, PhysicalRect) -> PhysicalRect>,
    ) {
        let handler = self.handler;
        self.record(
            self.bounds,
            self.clip,
            key,
            Call { value, draw, reads },
            opaque.unwrap_or_default(),
            move |input, offset, command, _, _, _, target| {
                let call = ErasedCommand {
                    handler,
                    value: offset,
                    opaque: Offset::new(0),
                    prepare: None,
                    draw: Some(|handler, offset, _, cx| {
                        // safety: recording pairs this offset with Call and this handler with H
                        let call = unsafe { cx.list.data().get(Offset::<Call<H, T, P>>::new(offset)) };
                        (call.draw)(unsafe { handler.cast::<H>().as_mut() }, &call.value, cx);
                    }),
                    reads: input.reads.map(|_| {
                        (|handler: NonNull<()>, offset, list: &DisplayList, output| {
                            // safety: recording pairs this offset with Call
                            let call = unsafe { list.data().get(Offset::<Call<H, T, P>>::new(offset)) };
                            // safety: recording selects the handler for H
                            call.reads.unwrap()(unsafe { handler.cast::<H>().as_ref() }, &call.value, output)
                        }) as _
                    }),
                };
                target.push(call, command.bounds, command.clip);
            },
        );
    }

    fn record<T: 'static, K: PartialEq + Hash + 'static, F>(
        &mut self,
        bounds: PhysicalRect,
        clip: u32,
        key: K,
        value: T,
        opaque: PhysicalRect,
        prepare: F,
    ) where
        F: Fn(&T, u32, &Command, &DisplayList, f32, &blit_arena::Arena<StoredImage>, &mut RasterCommands<P>) + 'static,
    {
        self.record_input(
            bounds,
            clip,
            (key, value, prepare, opaque),
            |(a, _, _, _), _, (b, _, _, _), _| a == b,
            |(key, _, _, _), _, state| key.hash(state),
            |(_, value, prepare, opaque), offset, command, list, context| {
                let index = context.commands.len() as usize;
                let value_offset = offset + offset_of!((K, T, F, PhysicalRect), 1) as u32;
                prepare(
                    value,
                    value_offset,
                    command,
                    list,
                    context.scale,
                    context.images,
                    context.commands,
                );
                if opaque.intersection(command.bounds).is_some() {
                    context.commands.metadata[index].coverage = Coverage::Rect;
                    context.commands.entries[index].opaque =
                        Offset::new(offset + offset_of!((K, T, F, PhysicalRect), 3) as u32);
                }
            },
        );
    }

    fn record_input<T: 'static, E, K, F>(
        &mut self,
        bounds: PhysicalRect,
        clip: u32,
        value: T,
        equal: E,
        hash: K,
        prepare: F,
    ) where
        E: Fn(&T, &DisplayList, &T, &DisplayList) -> bool + 'static,
        K: Fn(&T, &DisplayList, &mut FxHasher) + 'static,
        F: Fn(&T, u32, &Command, &DisplayList, &mut PrepareContext<'_, P>) + 'static,
    {
        let ops = const {
            &Ops {
                kind: TypeId::of::<(T, E, K, F)>(),
                equal: |left, a, right, b| {
                    // safety: insertion pairs this table with the stored tuple
                    let (a, equal, _, _) = unsafe { left.data().get(Offset::<(T, E, K, F)>::new(a)) };
                    let (b, _, _, _) = unsafe { right.data().get(Offset::<(T, E, K, F)>::new(b)) };
                    equal(a, left, b, right)
                },
                hash: |list, offset, state| {
                    // safety: insertion pairs this table with the stored tuple
                    let (value, _, hash, _) = unsafe { list.data().get(Offset::<(T, E, K, F)>::new(offset)) };
                    hash(value, list, state);
                },
                dispatch: |command, list, context| {
                    // safety: recording pairs this table with its input and pixel type
                    let (value, _, _, prepare) =
                        unsafe { list.data().get(Offset::<(T, E, K, F)>::new(command.offset())) };
                    let offset = command.offset() + offset_of!((T, E, K, F), 0) as u32;
                    let context = unsafe { context.cast::<PrepareContext<'_, P>>().as_mut() };
                    prepare(value, offset, command, list, context);
                },
            }
        };
        // safety: the table matches the stored tuple and this renderer's pixel type
        unsafe { self.list.push(bounds, clip, (value, equal, hash, prepare), ops) };
    }
}

struct StoredRectangle {
    rectangle: Rectangle<'static>,
    stops: [Option<Slice<GradientStop>>; 2],
}

impl StoredRectangle {
    fn view<'a>(&self, list: &'a DisplayList) -> Rectangle<'a> {
        let mut rectangle = self.rectangle;
        for (fill, stops) in [&mut rectangle.background, &mut rectangle.border.fill]
            .into_iter()
            .zip(self.stops)
        {
            if let Fill::LinearGradient(gradient) = fill {
                gradient.stops = list.slice(stops.unwrap());
            }
        }
        rectangle
    }
}

struct StoredText {
    request: TextRequest<'static>,
    colors: Slice<blit_gui::color::Color>,
}

impl StoredText {
    fn view<'a>(&self, list: &'a DisplayList) -> TextRequest<'a> {
        TextRequest {
            colors: list.slice(self.colors),
            ..self.request
        }
    }
}

struct StoredMesh {
    vertices: Slice<MeshVertex>,
    indices: Slice<u32>,
}

impl StoredMesh {
    fn view<'a>(&self, list: &'a DisplayList) -> Mesh<'a> {
        Mesh {
            vertices: list.slice(self.vertices),
            indices: list.slice(self.indices),
        }
    }
}

#[derive(Clone, Copy)]
pub struct ErasedCommand<P> {
    handler: NonNull<()>,
    value: u32,
    pub opaque: Offset<PhysicalRect>,
    prepare: Option<fn(NonNull<()>, u32, &Bump, &mut Bump, &mut DrawContext<'_, P>) -> u32>,
    pub draw: Option<fn(NonNull<()>, u32, &Bump, &mut DrawContext<'_, P>)>,
    pub reads: Option<fn(NonNull<()>, u32, &DisplayList, PhysicalRect) -> PhysicalRect>,
}

impl<P> ErasedCommand<P> {
    pub fn prepared<H: 'static, T: 'static, R: 'static, F: Fn(&mut H, &T, &mut DrawContext<'_, P>) -> R + 'static>(
        data: &mut Bump,
        handler: NonNull<()>,
        value: T,
        prepare: F,
    ) -> Self
    where
        P: Pixel,
    {
        Self {
            handler,
            draw: None,
            reads: None,
            value: data.push((value, prepare)).byte(),
            opaque: Offset::new(0),
            prepare: Some(|handler, value, data, prepared, cx| {
                // safety: recording pairs this callback with its typed value
                let (value, prepare) = unsafe { data.get(Offset::<(T, F)>::new(value)) };
                // safety: recording pairs the handler pointer with its typed preparation
                prepared
                    .push(prepare(unsafe { handler.cast::<H>().as_mut() }, value, cx))
                    .byte()
            }),
        }
    }

    pub fn ready<T: 'static>(handler: NonNull<()>, prepared: &mut Bump, value: T) -> Self {
        Self {
            handler,
            value: prepared.push(value).byte(),
            opaque: Offset::new(0),
            prepare: None,
            draw: None,
            reads: None,
        }
    }

    pub fn is_pending(&self) -> bool {
        self.prepare.is_some()
    }

    pub unsafe fn get<'a, T: 'static>(&self, prepared: &'a Bump) -> &'a T {
        // safety: the caller selects the output type of this command
        unsafe { prepared.get(Offset::<T>::new(self.value)) }
    }

    #[inline(always)]
    pub fn prepare(&mut self, data: &Bump, prepared: &mut Bump, cx: &mut DrawContext<'_, P>) {
        if let Some(prepare) = self.prepare {
            self.value = prepare(self.handler, self.value, data, prepared, cx);
            self.prepare = None;
        }
    }

    pub fn draw(&mut self, data: &Bump, prepared: &mut Bump, cx: &mut DrawContext<'_, P>) {
        self.prepare(data, prepared, cx);
        if let Some(draw) = self.draw {
            draw(self.handler, self.value, prepared, cx);
        }
    }

    pub fn reads(&self, list: &DisplayList, output: PhysicalRect) -> PhysicalRect {
        (self.reads.unwrap())(self.handler, self.value, list, output)
    }
}

struct Call<H, T, P> {
    value: T,
    draw: fn(&mut H, &T, &mut DrawContext<'_, P>),
    reads: Option<fn(&H, &T, PhysicalRect) -> PhysicalRect>,
}

impl<H: 'static, T: 'static, P: Pixel> Call<H, T, P> {
    fn draw(handler: NonNull<()>, value: u32, data: &Bump, cx: &mut DrawContext<'_, P>) {
        // safety: recording pairs this callback with its typed value
        let call = unsafe { data.get(Offset::<Self>::new(value)) };
        // safety: recording pairs the handler pointer with this typed callback
        (call.draw)(unsafe { handler.cast::<H>().as_mut() }, &call.value, cx);
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

impl<P: Pixel> Handler<blit_gui::paint::Clear, P> for () {
    fn handle(&mut self, _: blit_gui::paint::Clear, cx: &mut PaintContext<'_, Self, P>) {
        cx.clear();
    }
}

pub struct PrepareContext<'a, P: Pixel> {
    pub scale: f32,
    pub images: &'a blit_arena::Arena<StoredImage>,
    pub commands: &'a mut RasterCommands<P>,
}
