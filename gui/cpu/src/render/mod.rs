mod blur;
mod context;
pub mod shadow;

use std::ptr::NonNull;

use blit::{
    PhysicalRect,
    internal::bump::{Bump, Offset},
};
use blit_gui::display_list::{Compare, DisplayList};
pub(crate) use context::RenderTarget;
pub use context::{Image, RenderContext, Span, Surface};

pub use crate::raster::rounded;
use crate::{
    Pixel, StoredImage,
    strategy::{clip::ClipStack, command::RasterCommands},
};

/// prepares recorded inputs lazily and renders their visible output
///
/// ```
/// use blit::{LogicalRect, Paint};
/// use blit_gui::damage::DamageTracker;
/// use blit_cpu::{RenderContext, Render, Pixel, PixelBuffer, PremultipliedRgbaColor, Renderer, RenderStrategy};
/// use blit_gui::color::Color;
///
/// struct Swatch(Color);
/// #[derive(Default)]
/// struct SwatchRenderer;
///
/// impl<P: Pixel> Render<P> for SwatchRenderer {
///     type Input = Color;
///     type Prepared = Color;
///
///     fn prepare(&mut self, color: &Color, _: &mut RenderContext<'_, P>) -> Color { *color }
///
///     fn render(&mut self, color: &Color, cx: &mut RenderContext<'_, P>) {
///         cx.for_each_span(|span| {
///             P::blend_slice(span.pixels, PremultipliedRgbaColor::new(*color, span.coverage));
///         });
///     }
/// }
///
/// impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<Swatch> for Renderer<B, S, D> {
///     fn paint(&mut self, bounds: LogicalRect, swatch: Swatch) {
///         self.record(bounds).push_ready::<SwatchRenderer>(swatch.0, None);
///     }
/// }
/// ```
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
    pub opaque: Offset<PhysicalRect>,
    pub prepare: Option<fn(NonNull<()>, u32, &mut Bump, &mut RenderContext<'_, P>) -> u32>,
    pub render: Option<fn(NonNull<()>, u32, &Bump, &mut RenderContext<'_, P>)>,
    pub reads: Option<fn(NonNull<()>, u32, &DisplayList, PhysicalRect) -> PhysicalRect>,
}

impl<P> ErasedCommand<P> {
    /// stores an already prepared value when no preparation callback is needed
    pub fn ready<T: 'static>(prepared: &mut Bump, value: T) -> Self {
        Self {
            renderer: NonNull::dangling(),
            value: prepared.push(value).byte(),
            opaque: Offset::new(0),
            prepare: None,
            render: None,
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
    pub fn prepare(&mut self, prepared: &mut Bump, cx: &mut RenderContext<'_, P>) {
        if let Some(prepare) = self.prepare {
            self.value = prepare(self.renderer, self.value, prepared, cx);
            self.prepare = None;
        }
    }

    pub fn render(&mut self, prepared: &mut Bump, cx: &mut RenderContext<'_, P>) {
        self.prepare(prepared, cx);
        if let Some(render) = self.render {
            render(self.renderer, self.value, prepared, cx);
        }
    }

    pub fn reads(&self, list: &DisplayList, output: PhysicalRect) -> PhysicalRect {
        (self.reads.unwrap())(self.renderer, self.value, list, output)
    }
}

pub(crate) struct DispatchContext<'a, P: Pixel> {
    pub screen: PhysicalRect,
    pub clips: &'a ClipStack,
    pub images: &'a blit_arena::Arena<StoredImage>,
    pub commands: &'a mut RasterCommands<P>,
}
