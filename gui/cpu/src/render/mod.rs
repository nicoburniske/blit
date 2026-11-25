mod blur;
mod context;
pub mod path;
pub mod shadow;

use core::ptr::NonNull;

use blit::{FrameStore, LogicalRect, PhysicalRect};
use blit_gui::display_list::{Compare, DisplayList};
pub(crate) use context::RenderTarget;
pub use context::{Image, RenderContext, Span, Surface};

pub use crate::raster::rounded;
use crate::{
    Pixel, StoredImage,
    strategy::{clip::ClipStack, command::RasterCommands},
};

/// drawing facilities for a recorded input
pub struct DrawContext<'a, P: Pixel> {
    pub scale: f32,
    pub area: LogicalRect,
    pub bounds: PhysicalRect,
    pub(crate) clip: u32,
    pub(crate) offset: u32,
    pub(crate) screen: PhysicalRect,
    pub(crate) clips: &'a ClipStack,
    pub(crate) images: &'a blit_util::arena::Arena<StoredImage>,
    pub(crate) list: &'a DisplayList,
    pub(crate) commands: &'a mut RasterCommands<P>,
}

impl<P: Pixel> DrawContext<'_, P> {
    pub fn store(&self) -> &FrameStore {
        self.list.store()
    }

    /// queues a draw over the visible spans of this command
    ///
    /// ```
    /// use blit::LogicalRect;
    /// use blit_cpu::{Pixel, PixelBuffer, PremultipliedRgbaColor, Renderer, RenderStrategy};
    /// use blit_gui::{color::Color, damage::DamageTracker};
    ///
    /// fn swatch<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker>(
    ///     renderer: &mut Renderer<B, S, D>,
    ///     area: LogicalRect,
    ///     color: Color,
    /// ) {
    ///     renderer.record(area).push(color, |color, cx| {
    ///         cx.rasterize(*color, |color, cx| {
    ///             cx.for_each_span(|span| {
    ///                 B::Pixel::blend_slice(span.pixels, PremultipliedRgbaColor::new(*color, span.coverage));
    ///             });
    ///         });
    ///     });
    /// }
    /// ```
    pub fn rasterize<T, F>(&mut self, value: T, draw: F)
    where
        T: 'static,
        F: Fn(&T, &mut RenderContext<'_, P>) + 'static,
    {
        let mut call = ErasedCommand::ready(&mut self.commands.prepared, (value, draw));
        call.render = Some(|_, offset, prepared, cx| {
            // safety: this entry stores the value and callback together
            let (value, draw) = unsafe { prepared.get::<(T, F)>(offset) };
            draw(value, cx);
        });
        self.commands.push(call, self.bounds, self.clip);
    }
}

/// prepares recorded inputs lazily and renders their visible output
///
/// skips unchanged or hidden pixels
/// external changes require [`crate::Renderer::invalidate_all`]
///
/// ```compile_fail
/// use blit::{LogicalRect, Paint};
/// use blit_cpu::{Renderer, VecBuffer, Xrgb8888};
/// struct Unsupported;
/// fn paint(renderer: &mut Renderer<VecBuffer<Xrgb8888>>) {
///     renderer.paint(LogicalRect::default(), Unsupported);
/// }
/// ```
pub trait Render<P>: Sized + 'static
where
    P: Pixel,
{
    type Input: Compare;
    type Prepared: 'static;

    fn prepare(&mut self, input: &Self::Input, cx: &mut RenderContext<'_, P>) -> Self::Prepared;
    fn render(&mut self, prepared: &Self::Prepared, cx: &mut RenderContext<'_, P>);

    /// declares the source footprint for a framebuffer pass
    fn reads(&self, _: &Self::Input, output: PhysicalRect) -> PhysicalRect {
        output
    }

    /// called after each frame once the renderer has been used
    fn finish_frame(&mut self) {}
}

#[derive(Clone, Copy)]
pub(crate) struct ErasedCommand<P> {
    pub renderer: NonNull<()>,
    pub value: u32,
    pub opaque: u32,
    pub prepare: Option<fn(NonNull<()>, u32, &mut FrameStore, &mut RenderContext<'_, P>) -> u32>,
    pub render: Option<fn(NonNull<()>, u32, &FrameStore, &mut RenderContext<'_, P>)>,
    pub reads: Option<fn(NonNull<()>, u32, &DisplayList, PhysicalRect) -> PhysicalRect>,
}

impl<P> ErasedCommand<P> {
    /// stores an already prepared value when no preparation callback is needed
    pub fn ready<T: 'static>(prepared: &mut FrameStore, value: T) -> Self {
        Self {
            renderer: NonNull::dangling(),
            value: prepared.push(value),
            opaque: 0,
            prepare: None,
            render: None,
            reads: None,
        }
    }

    pub fn is_pending(&self) -> bool {
        self.prepare.is_some()
    }

    pub unsafe fn get<'a, T: 'static>(&self, prepared: &'a FrameStore) -> &'a T {
        // safety: the caller selects the output type of this command
        unsafe { prepared.get::<T>(self.value) }
    }

    #[inline(always)]
    pub fn prepare(&mut self, prepared: &mut FrameStore, cx: &mut RenderContext<'_, P>) {
        if let Some(prepare) = self.prepare {
            self.value = prepare(self.renderer, self.value, prepared, cx);
            self.prepare = None;
        }
    }

    pub fn render(&mut self, prepared: &mut FrameStore, cx: &mut RenderContext<'_, P>) {
        self.prepare(prepared, cx);
        if let Some(render) = self.render {
            render(self.renderer, self.value, prepared, cx);
        }
    }

    pub fn reads(&self, list: &DisplayList, output: PhysicalRect) -> PhysicalRect {
        (self.reads.unwrap())(self.renderer, self.value, list, output)
    }
}

pub(crate) type Replay<P> = fn(usize, &mut DrawContext<'_, P>);
