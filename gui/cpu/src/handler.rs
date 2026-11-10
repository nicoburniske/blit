use std::{marker::PhantomData, mem::offset_of, ptr::NonNull};

use blit::{
    LogicalRect, PhysicalRect,
    internal::{
        bump::{Bump, Offset},
        hash::FxHasher,
    },
};
use blit_gui::{
    Image,
    display_list::{Command, Compare, DisplayList, Slice},
    image::{ImageData, ImageHandle},
};

use crate::{
    DrawContext, Pixel, StoredImage,
    strategy::command::{Coverage, RasterCommands},
};

/// prepares recorded inputs lazily and draws their visible output
///
/// ```
/// use blit::{LogicalRect, Paint};
/// use blit_gui::damage::DamageTracker;
/// use blit_cpu::{DrawContext, Handler, Pixel, PixelBuffer, PremultipliedRgbaColor, Renderer, RenderStrategy};
/// use blit_gui::color::Color;
///
/// struct Swatch(Color);
/// #[derive(Default)]
/// struct Painter;
///
/// impl<P: Pixel> Handler<Color, P> for Painter {
///     type Prepared = Color;
///
///     fn prepare(&mut self, color: &Color, _: &mut DrawContext<'_, P>) -> Color { *color }
///
///     fn draw(&mut self, color: &Color, cx: &mut DrawContext<'_, P>) {
///         cx.for_each_span(|span| {
///             P::blend_slice(span.pixels, PremultipliedRgbaColor::new(*color, span.coverage));
///         });
///     }
/// }
///
/// impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<Swatch> for Renderer<B, S, D> {
///     fn paint(&mut self, bounds: LogicalRect, swatch: Swatch) {
///         self.record::<Painter>(bounds).draw_ready(swatch.0, None);
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
pub trait Handler<T, P>: Sized + 'static
where
    P: Pixel,
{
    type Prepared: 'static;

    fn prepare(&mut self, input: &T, cx: &mut DrawContext<'_, P>) -> Self::Prepared;
    fn draw(&mut self, prepared: &Self::Prepared, cx: &mut DrawContext<'_, P>);

    /// declares the source footprint for a framebuffer pass
    fn reads(&self, _: &T, output: PhysicalRect) -> PhysicalRect {
        output
    }

    /// called after each frame once the handler has been used
    fn finish_frame(&mut self) {}
}

pub struct PaintContext<'a, H, P: Pixel> {
    pub scale: f32,
    pub bounds: PhysicalRect,
    pub(crate) clip: u32,
    pub(crate) handlers: &'a mut Vec<crate::ErasedHandler>,
    pub(crate) marker: PhantomData<fn(H, P) -> (H, P)>,
    pub(crate) list: &'a mut DisplayList,
    pub(crate) reads_framebuffer: &'a mut bool,
    pub(crate) images: &'a mut blit_arena::Arena<StoredImage>,
}

impl<H, P> PaintContext<'_, H, P>
where
    H: Default + 'static,
    P: Pixel,
{
    /// prepares an image with an explicit logical to physical scale
    pub fn prepare_image(&self, area: LogicalRect, request: &Image, scale: f32) -> Option<crate::raster::Image> {
        crate::raster::Image::new(area, request, scale, self.bounds, self.images)
    }

    pub fn store_slice<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> Slice<T> {
        self.list.store_slice(values)
    }

    /// records an already prepared value without running preparation
    /// opaque guarantees fully opaque output inside a physical rectangle intersected with bounds and clip
    #[inline]
    pub fn draw_ready<T>(&mut self, value: T, opaque: Option<PhysicalRect>)
    where
        T: Compare,
        H: Handler<T, P, Prepared = T>,
    {
        let handler = self.handler::<T>();
        self.record_draw(value, opaque, handler, false);
    }

    /// prepares once on the first visible span and reuses the result for the frame
    /// opaque guarantees fully opaque output inside a physical rectangle intersected with bounds and clip
    pub fn draw<T>(&mut self, value: T, opaque: Option<PhysicalRect>)
    where
        T: Compare,
        H: Handler<T, P>,
    {
        let handler = self.handler::<T>();
        self.record(
            self.bounds,
            self.clip,
            value,
            opaque.unwrap_or_default(),
            move |_, offset, command, _, _, _, target| {
                let call = ErasedCommand {
                    draw: Some(|handler, offset, prepared, cx| {
                        // safety: recording pairs this value and handler with H
                        let value = unsafe { prepared.get(Offset::<H::Prepared>::new(offset)) };
                        unsafe { handler.cast::<H>().as_mut() }.draw(value, cx);
                    }),
                    ..ErasedCommand::prepared(&mut target.data, handler, offset, |handler: &mut H, offset, cx| {
                        // safety: recording pairs this offset with T
                        let value = unsafe { cx.list.data().get(Offset::<T>::new(*offset)) };
                        handler.prepare(value, cx)
                    })
                };
                target.push(call, command.bounds, command.clip);
            },
        );
    }

    /// records a framebuffer effect with its source footprint for each output region
    pub fn pass<T>(&mut self, value: T)
    where
        T: Compare,
        H: Handler<T, P, Prepared = T>,
    {
        let handler = self.handler::<T>();
        *self.reads_framebuffer = true;
        self.record_draw(value, None, handler, true);
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        let id = self
            .images
            .insert_with(|id| StoredImage::new(ImageHandle::new(id.into(), data.size), data));
        self.images.get(id).unwrap().handle.clone()
    }

    fn record_draw<T>(&mut self, value: T, opaque: Option<PhysicalRect>, handler: NonNull<()>, reads: bool)
    where
        T: Compare,
        H: Handler<T, P, Prepared = T>,
    {
        self.record(
            self.bounds,
            self.clip,
            value,
            opaque.unwrap_or_default(),
            move |_, offset, command, _, _, _, target| {
                let call = ErasedCommand {
                    handler,
                    value: offset,
                    opaque: Offset::new(0),
                    prepare: None,
                    draw: Some(|handler, offset, _, cx| {
                        // safety: recording pairs this value and handler with H
                        let value = unsafe { cx.list.data().get(Offset::<T>::new(offset)) };
                        unsafe { handler.cast::<H>().as_mut() }.draw(value, cx);
                    }),
                    reads: reads.then_some({
                        (|handler: NonNull<()>, offset, list: &DisplayList, output| {
                            // safety: recording pairs this offset with T
                            let value = unsafe { list.data().get(Offset::<T>::new(offset)) };
                            // safety: recording selects the handler for H
                            unsafe { handler.cast::<H>().as_ref() }.reads(value, output)
                        }) as _
                    }),
                };
                target.push(call, command.bounds, command.clip);
            },
        );
    }

    fn handler<T>(&mut self) -> NonNull<()>
    where
        H: Handler<T, P>,
    {
        let index = crate::handler_index::<H>(self.handlers);
        let handler = &mut self.handlers[index];
        handler.finish = |handler| {
            // safety: recording selects the entry created for H
            unsafe { handler.cast::<H>() }.finish_frame();
        };
        handler.value
    }

    fn record<T, F>(&mut self, bounds: PhysicalRect, clip: u32, value: T, opaque: PhysicalRect, prepare: F)
    where
        T: Compare,
        H: Handler<T, P>,
        F: Fn(&T, u32, &Command, &DisplayList, f32, &blit_arena::Arena<StoredImage>, &mut RasterCommands<P>) + 'static,
    {
        self.record_input(
            bounds,
            clip,
            (value, prepare, opaque),
            |(a, _, _), left, (b, _, _), right| <T as Compare>::equal(a, left, b, right),
            |(input, _, _), list, state| <T as Compare>::hash(input, list, state),
            |(value, prepare, opaque), offset, command, list, context| {
                let index = context.commands.len() as usize;
                let value_offset = offset + offset_of!((T, F, PhysicalRect), 0) as u32;
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
                        Offset::new(offset + offset_of!((T, F, PhysicalRect), 2) as u32);
                }
            },
        );
    }

    pub(crate) fn record_input<T: 'static, E, K, F>(
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
        // safety: this renderer dispatches its list with PrepareContext
        unsafe { self.list.record(bounds, clip, value, equal, hash, prepare) };
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
    /// stores inputs and a callback when the value needs lazy preparation before drawing
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

    /// stores an already prepared value when no preparation callback is needed
    pub fn ready<T: 'static>(prepared: &mut Bump, value: T) -> Self {
        Self {
            handler: NonNull::dangling(),
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

pub struct PrepareContext<'a, P: Pixel> {
    pub scale: f32,
    pub screen: PhysicalRect,
    pub images: &'a blit_arena::Arena<StoredImage>,
    pub commands: &'a mut RasterCommands<P>,
}
