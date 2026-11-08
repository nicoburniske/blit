//! experimental typed recording and rendering without a concrete gui context

use std::marker::PhantomData;

use blit::{Clip, ClipShape, LogicalRect, Paint, PhysicalRect, Scale2};
use blit_gui::{
    BoundsClip, PaintCustom, RenderInput, TextSystem,
    display_list::{ClipId, CustomCommand, DisplayList, Rectangle, arena::Writer},
    style::BorderRadius,
};

use crate::{DamageTracker, Direct, PixelBuffer, RenderStrategy, Renderer, TileDamage};

/// owns a frame's recording state and its configured renderer
///
/// a command needs a handler in the painter's configuration
///
/// ```compile_fail
/// use blit::{Paint, LogicalRect};
/// use blit_cpu::{VecBuffer, Xrgb8888, paint::Painter};
/// use blit_gui::{color::Color, shadow::BoxShadow};
/// fn draw(painter: &mut Painter<VecBuffer<Xrgb8888>>) {
///     let shadow = BoxShadow::new(LogicalRect::default(), Color::BLACK);
///     painter.paint(shadow.bounds(), shadow);
/// }
/// ```
///
/// arena builders require the same capability
///
/// ```compile_fail
/// use blit::LogicalRect;
/// use blit_cpu::{VecBuffer, Xrgb8888, paint::Painter};
/// use blit_gui::{PaintCustom, color::Color, shadow::BoxShadow};
/// fn draw(painter: &mut Painter<VecBuffer<Xrgb8888>>) {
///     let shadow = BoxShadow::new(LogicalRect::default(), Color::BLACK);
///     painter.paint_with(shadow.bounds(), |_| shadow);
/// }
/// ```
pub struct Painter<B: PixelBuffer, S: RenderStrategy<B> = Direct, D: DamageTracker = TileDamage, H = ()> {
    renderer: Renderer<B, S, D>,
    display_list: DisplayList,
    text: TextSystem,
    scale: f32,
    clip: ClipId,
    clips: Vec<ClipId>,
    handlers: PhantomData<fn() -> H>,
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker, H> Painter<B, S, D, H> {
    pub fn new(mut renderer: Renderer<B, S, D>, text: TextSystem, handlers: H) -> Self
    where
        H: Handlers<B, S, D>,
    {
        handlers.register(&mut renderer);
        Self {
            renderer,
            display_list: DisplayList::default(),
            text,
            scale: 1.0,
            clip: ClipId::default(),
            clips: Vec::new(),
            handlers: PhantomData,
        }
    }

    /// renders and clears the recorded frame while retaining resources and caches
    pub fn render(&mut self, buffer_age: u8) {
        assert!(self.clips.is_empty(), "unbalanced paint clips");
        self.renderer.render(
            RenderInput::new(&mut self.display_list, &mut self.text, self.scale),
            buffer_age,
        );
        self.display_list.clear();
        self.clip = ClipId::default();
        self.text.finish_frame();
    }

    pub fn push_clip(&mut self, area: LogicalRect, radius: BorderRadius) {
        self.clips.push(self.clip);
        self.clip = self.display_list.push_clip(self.clip, area, radius);
    }

    pub fn pop_clip(&mut self) {
        self.clip = self.clips.pop().expect("no paint clip to pop");
    }

    pub fn set_scale(&mut self, scale: f32) {
        assert!(scale.is_finite() && scale > 0.0);
        self.scale = scale;
    }

    pub fn buffer(&self) -> &B {
        self.renderer.buffer()
    }

    pub fn buffer_mut(&mut self) -> &mut B {
        self.renderer.buffer_mut()
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker, H> blit::Context for Painter<B, S, D, H> {
    type Scalar = f32;
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker, H: Commands<C>, C> Paint<C> for Painter<B, S, D, H> {
    fn paint(&mut self, bounds: LogicalRect, command: C) {
        H::record(
            &mut self.display_list,
            bounds.to_physical(Scale2::uniform(self.scale)),
            self.clip,
            command,
        );
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker, H: Commands<C>, C: CustomCommand> PaintCustom<C>
    for Painter<B, S, D, H>
{
    fn paint_with(&mut self, bounds: LogicalRect, build: impl FnOnce(&mut Writer<'_>) -> C) {
        self.display_list
            .push_custom(bounds.to_physical(Scale2::uniform(self.scale)), self.clip, build);
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker, H> Clip<Painter<B, S, D, H>> for BoundsClip {
    fn shape(&self, area: LogicalRect) -> ClipShape<f32> {
        ClipShape::Rect(area)
    }

    fn push(&self, context: &mut Painter<B, S, D, H>, area: LogicalRect) {
        context.push_clip(area, BorderRadius::default());
    }

    fn pop(&self, context: &mut Painter<B, S, D, H>) {
        context.pop_clip();
    }
}

#[doc(hidden)]
pub trait Handlers<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> {
    fn register(self, renderer: &mut Renderer<B, S, D>);
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Handlers<B, S, D> for () {
    fn register(self, _: &mut Renderer<B, S, D>) {}
}

#[doc(hidden)]
pub trait Commands<C> {
    fn record(list: &mut DisplayList, bounds: PhysicalRect, clip: ClipId, command: C);
}

impl<H> Commands<Rectangle<'_>> for H {
    fn record(list: &mut DisplayList, bounds: PhysicalRect, clip: ClipId, command: Rectangle<'_>) {
        list.push_rectangle(command, bounds, clip);
    }
}

#[doc(hidden)]
pub use {blit, blit_gui};

/// declares a handler configuration and its supported commands together
#[macro_export]
macro_rules! paint_handlers {
    (
        $(#[$attribute:meta])* $visibility:vis struct $name:ident {
            $($field:ident: $command:ty => $handler:ty),* $(,)?
        }
    ) => {
        $(#[$attribute])*
        $visibility struct $name {
            $($field: $handler),*
        }

        impl<B, S, D> $crate::paint::Handlers<B, S, D> for $name
        where
            B: $crate::PixelBuffer,
            S: $crate::RenderStrategy<B>,
            D: $crate::DamageTracker,
            $($handler: $crate::extension::Extension<B::Pixel, Command = $command>),*
        {
            fn register(self, renderer: &mut $crate::Renderer<B, S, D>) {
                $(renderer.register(self.$field);)*
            }
        }

        $(impl $crate::paint::Commands<$command> for $name {
            fn record(
                list: &mut $crate::paint::blit_gui::display_list::DisplayList,
                bounds: $crate::paint::blit::PhysicalRect,
                clip: $crate::paint::blit_gui::display_list::ClipId,
                command: $command,
            ) {
                list.push_custom(bounds, clip, |_| command);
            }
        })*
    };
}
