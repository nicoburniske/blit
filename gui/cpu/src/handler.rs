use std::{hash::Hasher, marker::PhantomData, mem::offset_of, ptr::NonNull};

use blit::{
    PhysicalRect,
    internal::bump::{Bump, Offset},
};
use blit_gui::{
    display_list::{Command, Compare, DisplayList, Slice},
    image::{ImageData, ImageHandle},
};

use crate::{
    DrawContext, Pixel, StoredImage,
    strategy::{
        clip::ClipStack,
        command::{Coverage, RasterCommands},
    },
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
/// impl<P: Pixel> Handler<P> for Painter {
///     type Input = Color;
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
pub trait Handler<P>: Sized + 'static
where
    P: Pixel,
{
    type Input: Compare;
    type Prepared: 'static;

    fn prepare(&mut self, input: &Self::Input, cx: &mut DrawContext<'_, P>) -> Self::Prepared;
    fn draw(&mut self, prepared: &Self::Prepared, cx: &mut DrawContext<'_, P>);

    /// declares the source footprint for a framebuffer pass
    fn reads(&self, _: &Self::Input, output: PhysicalRect) -> PhysicalRect {
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
    pub fn store_slice<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> Slice<T> {
        self.list.store_slice(values)
    }

    /// records an already prepared value without running preparation
    /// opaque guarantees fully opaque output inside a physical rectangle intersected with bounds and clip
    #[inline]
    pub fn draw_ready(&mut self, value: H::Input, opaque: Option<PhysicalRect>)
    where
        H: Handler<P, Prepared = <H as Handler<P>>::Input>,
    {
        let handler = self.handler();
        self.record_draw::<false>(value, opaque, handler);
    }

    /// prepares once on the first visible span and reuses the result for the frame
    /// opaque guarantees fully opaque output inside a physical rectangle intersected with bounds and clip
    pub fn draw(&mut self, value: H::Input, opaque: Option<PhysicalRect>)
    where
        H: Handler<P>,
    {
        let handler = self.handler();
        self.record(value, opaque.unwrap_or_default(), move |offset, command, target| {
            let call = ErasedCommand {
                handler,
                value: offset,
                opaque: Offset::new(0),
                reads: None,
                prepare: Some(|handler, offset, prepared, cx| {
                    // safety: recording pairs this offset with H::Input and the handler with H
                    let value = unsafe { cx.list.data().get(Offset::<H::Input>::new(offset)) };
                    let handler = unsafe { handler.cast::<H>().as_mut() };
                    prepared.push(handler.prepare(value, cx)).byte()
                }),
                draw: Some(|handler, offset, prepared, cx| {
                    // safety: recording pairs this value and handler with H
                    let value = unsafe { prepared.get(Offset::<H::Prepared>::new(offset)) };
                    unsafe { handler.cast::<H>().as_mut() }.draw(value, cx);
                }),
            };
            target.push(call, command.bounds, command.clip);
        });
    }

    /// records a framebuffer effect with its source footprint for each output region
    pub fn pass(&mut self, value: H::Input)
    where
        H: Handler<P, Prepared = <H as Handler<P>>::Input>,
    {
        let handler = self.handler();
        *self.reads_framebuffer = true;
        self.record_draw::<true>(value, None, handler);
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        let id = self.images.insert_with(|id| StoredImage::new(id.into(), data));
        self.images.get(id).unwrap().handle.clone()
    }

    fn record_draw<const READS: bool>(&mut self, value: H::Input, opaque: Option<PhysicalRect>, handler: NonNull<()>)
    where
        H: Handler<P, Prepared = <H as Handler<P>>::Input>,
    {
        self.record(value, opaque.unwrap_or_default(), move |offset, command, target| {
            let call = ErasedCommand {
                handler,
                value: offset,
                opaque: Offset::new(0),
                prepare: None,
                draw: Some(|handler, offset, _, cx| {
                    // safety: recording pairs this value and handler with H
                    let value = unsafe { cx.list.data().get(Offset::<H::Input>::new(offset)) };
                    unsafe { handler.cast::<H>().as_mut() }.draw(value, cx);
                }),
                reads: READS.then_some({
                    (|handler: NonNull<()>, offset, list: &DisplayList, output| {
                        // safety: recording pairs this offset with H::Input
                        let value = unsafe { list.data().get(Offset::<H::Input>::new(offset)) };
                        // safety: recording selects the handler for H
                        unsafe { handler.cast::<H>().as_ref() }.reads(value, output)
                    }) as _
                }),
            };
            target.push(call, command.bounds, command.clip);
        });
    }

    fn handler(&mut self) -> NonNull<()>
    where
        H: Handler<P>,
    {
        let index = crate::handler_index::<H>(self.handlers);
        let handler = &mut self.handlers[index];
        handler.finish = |handler| {
            // safety: recording selects the entry created for H
            unsafe { handler.cast::<H>() }.finish_frame();
        };
        handler.value
    }

    fn record<F>(&mut self, value: H::Input, opaque: PhysicalRect, prepare: F)
    where
        H: Handler<P>,
        F: Fn(u32, &Command, &mut RasterCommands<P>) + 'static,
    {
        self.record_input(
            self.bounds,
            self.clip,
            Recorded { value, prepare, opaque },
            |input, offset, command, _, context| {
                let index = context.commands.len() as usize;
                let value_offset = offset + offset_of!(Recorded<H::Input, F>, value) as u32;
                (input.prepare)(value_offset, command, context.commands);
                if input.opaque.intersection(command.bounds).is_some() {
                    context.commands.metadata[index].coverage = Coverage::Rect;
                    context.commands.entries[index].opaque =
                        Offset::new(offset + offset_of!(Recorded<H::Input, F>, opaque) as u32);
                }
            },
        );
    }

    pub(crate) fn record_input<T: Compare, F>(&mut self, bounds: PhysicalRect, clip: u32, value: T, prepare: F)
    where
        F: Fn(&T, u32, &Command, &DisplayList, &mut PrepareContext<'_, P>) + 'static,
    {
        // safety: this renderer dispatches its list with PrepareContext
        unsafe { self.list.record(bounds, clip, value, prepare) };
    }
}

struct Recorded<T, F> {
    value: T,
    prepare: F,
    opaque: PhysicalRect,
}

impl<T: Compare, F: 'static> Compare for Recorded<T, F> {
    fn equal(a: &Self, left: &DisplayList, b: &Self, right: &DisplayList) -> bool {
        T::equal(&a.value, left, &b.value, right)
    }

    fn hash(input: &Self, list: &DisplayList, state: &mut impl Hasher) {
        T::hash(&input.value, list, state);
    }
}

#[derive(Clone, Copy)]
pub struct ErasedCommand<P> {
    pub handler: NonNull<()>,
    pub value: u32,
    pub opaque: Offset<PhysicalRect>,
    pub prepare: Option<fn(NonNull<()>, u32, &mut Bump, &mut DrawContext<'_, P>) -> u32>,
    pub draw: Option<fn(NonNull<()>, u32, &Bump, &mut DrawContext<'_, P>)>,
    pub reads: Option<fn(NonNull<()>, u32, &DisplayList, PhysicalRect) -> PhysicalRect>,
}

impl<P> ErasedCommand<P> {
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
    pub fn prepare(&mut self, prepared: &mut Bump, cx: &mut DrawContext<'_, P>) {
        if let Some(prepare) = self.prepare {
            self.value = prepare(self.handler, self.value, prepared, cx);
            self.prepare = None;
        }
    }

    pub fn draw(&mut self, prepared: &mut Bump, cx: &mut DrawContext<'_, P>) {
        self.prepare(prepared, cx);
        if let Some(draw) = self.draw {
            draw(self.handler, self.value, prepared, cx);
        }
    }

    pub fn reads(&self, list: &DisplayList, output: PhysicalRect) -> PhysicalRect {
        (self.reads.unwrap())(self.handler, self.value, list, output)
    }
}

pub struct PrepareContext<'a, P: Pixel> {
    pub screen: PhysicalRect,
    pub clips: &'a ClipStack,
    pub images: &'a blit_arena::Arena<StoredImage>,
    pub commands: &'a mut RasterCommands<P>,
}
