mod damage;
mod glyph;
mod paint;
mod pixel;
pub mod raster;
mod render;
pub mod shadow;
mod strategy;
mod text;

use std::{any::TypeId, marker::PhantomData, ptr::NonNull};

use blit::{LogicalRect, Paint, PhysicalRect, Scale2};
use blit_arena::Arena;
use blit_gui::{
    Context, TextSystem,
    damage::{DamageTracker, tile::TileDamage},
    display_list::{self, DisplayList},
    image::{ImageData, ImageHandle},
    paint::{ImageRequest, Mesh, Rectangle, TextRequest},
    style::BorderRadius,
};

use self::{
    damage::passes::{PassDamage, StageDamage},
    pixel::PixelSpan,
    raster::DrawTarget,
    render::image::StoredImage,
    strategy::{clip::ClipStack, command::RasterCommands},
    text::TextRenderer,
};
pub use self::{
    paint::{Handler, PaintContext},
    pixel::{Argb8888, Pixel, PixelBuffer, PremultipliedRgbaColor, Rgb8Pixel, Rgba8888, VecBuffer, Xrgb8888},
    raster::{DrawContext, Span, Surface},
    strategy::{Direct, RenderStrategy, Scanline},
};

pub struct Renderer<B: PixelBuffer, S: RenderStrategy<B> = Direct, D: DamageTracker = TileDamage> {
    context: RenderContext<B>,
    handlers: Vec<ErasedHandler>,
    paint_clip: u32,
    paint_clips: Vec<u32>,
    strategy: S,
    damage: D,
    pass_damage: PassDamage,
    pass_shift: u32,
}

impl<B: PixelBuffer> Renderer<B> {
    pub fn new(buffer: B, text: TextSystem, config: RendererConfig) -> Self {
        Self {
            context: RenderContext {
                buffer,
                scale_factor: 1.0,
                images: Arena::new(),
                text: TextRenderer::new(&config, text),
                list: DisplayList::default(),
                raster: RasterCommands::default(),
                clips: ClipStack::default(),
                snapshot: Surface {
                    width: 0,
                    height: 0,
                    pixels: Vec::new(),
                    valid: damage::region::Region::default(),
                },
                preserved: Vec::new(),
            },
            handlers: Vec::new(),
            paint_clip: 0,
            paint_clips: Vec::new(),
            strategy: Direct::default(),
            damage: TileDamage::new(config.damage_tile_size),
            pass_damage: PassDamage::default(),
            pass_shift: config.damage_tile_size.trailing_zeros(),
        }
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Renderer<B, S, D> {
    /// configures one handler before recording begins
    pub fn handler<H: 'static>(mut self, handler: H) -> Self {
        assert!(
            self.context.list.commands().is_empty(),
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
            self.context.list.commands().is_empty(),
            "cannot configure a handler during a frame"
        );
        self.damage.invalidate();
        let index = self.handler_index::<H>();
        // safety: the lookup selects the entry created for H
        unsafe { self.handlers[index].cast::<H>() }
    }

    /// dispatches drawing data to its handler and creates its default on first use
    pub fn handle<H: Handler<T, B::Pixel> + Default, T>(&mut self, bounds: LogicalRect, command: T) {
        let index = self.handler_index::<H>();
        let handler = &mut self.handlers[index];
        handler.finish = |handler| {
            // safety: dispatch pairs this callback with the entry created for H
            unsafe { handler.cast::<H>() }.finish_frame();
        };
        let pointer = handler.value;
        let context = &mut self.context;
        // safety: the lookup selects the entry created for H
        unsafe { handler.cast::<H>() }.handle(
            command,
            &mut PaintContext {
                scale: context.scale_factor,
                bounds: bounds.to_physical(Scale2::uniform(context.scale_factor)),
                clip: self.paint_clip,
                handler: pointer,
                marker: PhantomData,
                list: &mut context.list,
                images: &mut context.images,
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

    pub fn strategy<T: RenderStrategy<B>>(self, strategy: T) -> Renderer<B, T, D> {
        Renderer {
            context: self.context,
            handlers: self.handlers,
            paint_clip: self.paint_clip,
            paint_clips: self.paint_clips,
            strategy,
            damage: self.damage,
            pass_damage: self.pass_damage,
            pass_shift: self.pass_shift,
        }
    }

    pub fn damage_tracker<T: DamageTracker>(self, mut damage: T) -> Renderer<B, S, T> {
        damage.invalidate();
        Renderer {
            context: self.context,
            handlers: self.handlers,
            paint_clip: self.paint_clip,
            paint_clips: self.paint_clips,
            strategy: self.strategy,
            damage,
            pass_damage: self.pass_damage,
            pass_shift: self.pass_shift,
        }
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        data.validate();
        let id = self
            .context
            .images
            .insert_with(|id| StoredImage::new(ImageHandle::new(id.into(), data.size), data));
        self.context.images.get(id).unwrap().handle.clone()
    }

    /// buffer age counts presentations since this buffer was last used
    /// zero means its contents are unknown and forces a full repaint
    pub fn render(&mut self, buffer_age: u8) {
        assert!(self.paint_clips.is_empty(), "unbalanced paint clips");
        let screen = self.screen();
        let scale = self.context.scale_factor;
        self.damage.update(&self.context.list, screen, scale, buffer_age);
        if !self.damage.is_empty() {
            let context = &mut self.context;
            let mut preparation = paint::PrepareContext {
                scale,
                screen,
                images: &context.images,
                commands: &mut context.raster,
            };
            for command in context.list.commands() {
                // safety: recording pairs dispatch with this renderer's preparation context
                unsafe {
                    command.dispatch(&context.list, std::ptr::NonNull::from(&mut preparation).cast());
                }
            }
        }
        self.pass_damage
            .update(&self.context, &self.damage, screen, self.pass_shift);
        let plan = &self.pass_damage;
        let mut damage = StageDamage {
            tracker: &mut self.damage,
            region: plan.stages.first(),
        };
        let context = &mut self.context;
        if !damage.is_empty() {
            let end = context.raster.len();
            if let Some(region) = plan.stages.first() {
                assert_eq!(
                    context.buffer.x_offset(),
                    0,
                    "source reads require a complete framebuffer"
                );
                context.preserved.clear();
                for bounds in plan.preserve.rects() {
                    for y in bounds.y..bounds.y + bounds.height {
                        context.preserved.extend_from_slice(
                            context
                                .buffer
                                .read_line(y as usize, bounds.x as usize..(bounds.x + bounds.width) as usize),
                        );
                    }
                }
                for bounds in region.rects() {
                    for y in bounds.y..bounds.y + bounds.height {
                        context.buffer.process_line(
                            y as usize,
                            bounds.x as usize..(bounds.x + bounds.width) as usize,
                            |pixels| pixels.fill(B::Pixel::background()),
                        );
                    }
                }
            }
            let mut start = 0;
            for (stage, node) in plan.passes.iter().enumerate() {
                context.raster.range = start..node.index;
                self.strategy.render(context, &damage);
                start = node.index + 1;
                damage.region = Some(&plan.stages[stage + 1]);
                let clip = context.raster.clip(node.index);
                if damage.intersects(node.bounds, clip) {
                    let snapshot = &mut context.snapshot;
                    snapshot.width = context.buffer.width();
                    snapshot.height = context.buffer.height();
                    snapshot
                        .pixels
                        .resize(snapshot.width * snapshot.height, B::Pixel::background());
                    snapshot.valid.clone_from(&node.reads);
                    for bounds in node.reads.rects() {
                        for y in bounds.y..bounds.y + bounds.height {
                            let range = bounds.x as usize..(bounds.x + bounds.width) as usize;
                            let start = y as usize * snapshot.width + range.start;
                            snapshot.pixels[start..start + range.len()]
                                .copy_from_slice(context.buffer.read_line(y as usize, range));
                        }
                    }
                    context.raster.entries[node.index as usize].draw(
                        &context.raster.data,
                        &mut context.raster.prepared,
                        &mut DrawContext {
                            bounds: node.bounds,
                            list: &context.list,
                            text: &mut context.text,
                            gradients: &mut context.raster.gradients,
                            images: &mut context.images,
                            source: Some(snapshot),
                            target: DrawTarget::Pass {
                                target: &mut context.buffer,
                                clips: &context.clips,
                                clip,
                                output: &plan.stages[stage + 1],
                            },
                        },
                    );
                }
            }
            context.raster.range = start..end;
            self.strategy.render(context, &damage);
            if !plan.passes.is_empty() {
                let mut offset = 0;
                for bounds in plan.preserve.rects() {
                    for y in bounds.y..bounds.y + bounds.height {
                        context.buffer.process_line(
                            y as usize,
                            bounds.x as usize..(bounds.x + bounds.width) as usize,
                            |pixels| {
                                pixels.copy_from_slice(&context.preserved[offset..offset + pixels.len()]);
                                offset += pixels.len();
                            },
                        );
                    }
                }
            }
        }
        context.raster.clear();
        context.clips.clear();
        let mut list = std::mem::take(&mut context.list);
        self.damage.finish_frame(&mut list);
        list.clear();
        context.list = list;
        context.text.finish_frame();
        for handler in &mut self.handlers {
            (handler.finish)(handler);
        }
        context.images.retain(|image| !image.handle.is_uniquely_owned());
        self.paint_clip = 0;
    }

    pub fn screen(&self) -> PhysicalRect {
        PhysicalRect::new(
            0,
            0,
            i32::try_from(self.context.buffer.width()).expect("buffer width exceeds physical coordinates"),
            i32::try_from(self.context.buffer.height()).expect("buffer height exceeds physical coordinates"),
        )
    }

    pub fn buffer(&self) -> &B {
        &self.context.buffer
    }
    pub fn buffer_mut(&mut self) -> &mut B {
        &mut self.context.buffer
    }
    pub fn invalidate_all(&mut self) {
        self.damage.invalidate();
    }

    pub fn set_scale(&mut self, scale: f32) {
        assert!(scale.is_finite() && scale > 0.0);
        assert!(
            self.context.list.commands().is_empty(),
            "cannot change scale during a frame"
        );
        if self.context.scale_factor != scale {
            self.context.scale_factor = scale;
            self.invalidate_all();
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

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<blit_gui::paint::Clear> for Renderer<B, S, D> {
    fn paint(&mut self, bounds: LogicalRect, command: blit_gui::paint::Clear) {
        self.handle::<(), _>(bounds, command);
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> blit::Context for Renderer<B, S, D> {
    type Scalar = f32;
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Context for Renderer<B, S, D> {
    fn text(&mut self) -> &mut TextSystem {
        &mut self.context.text.system
    }

    fn push_clip(&mut self, area: LogicalRect, radius: BorderRadius) {
        self.paint_clips.push(self.paint_clip);
        let clip = self.context.list.push_clip(display_list::Clip {
            parent: self.paint_clip,
            area,
            radius,
        });
        self.context
            .clips
            .push_node(self.paint_clip, area, radius, self.context.scale_factor);
        self.paint_clip = clip;
    }

    fn pop_clip(&mut self) {
        self.paint_clip = self.paint_clips.pop().expect("no paint clip to pop");
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RendererConfig {
    /// width and height of a damage tile in physical pixels as a nonzero power of two
    pub damage_tile_size: u16,
    /// maximum retained rasterized glyph data in bytes
    pub glyph_cache_capacity: usize,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            damage_tile_size: 32,
            glyph_cache_capacity: 1024 * 1024,
        }
    }
}

#[doc(hidden)]
pub struct RenderContext<B: PixelBuffer> {
    buffer: B,
    scale_factor: f32,
    images: Arena<StoredImage>,
    text: TextRenderer,
    list: DisplayList,
    raster: RasterCommands<B::Pixel>,
    clips: ClipStack,
    snapshot: Surface<B::Pixel>,
    preserved: Vec<B::Pixel>,
}

struct ErasedHandler {
    kind: TypeId,
    value: NonNull<()>,
    drop: fn(NonNull<()>),
    finish: fn(&mut Self),
}

impl ErasedHandler {
    fn new<H: 'static>(handler: H) -> Self {
        Self {
            kind: TypeId::of::<H>(),
            value: NonNull::from(Box::leak(Box::new(handler))).cast(),
            finish: |_| {},
            drop: |value| {
                // safety: new transfers ownership of one boxed H to this entry
                unsafe { drop(Box::from_raw(value.cast::<H>().as_ptr())) };
            },
        }
    }

    unsafe fn cast<H>(&mut self) -> &mut H {
        // safety: callers select the type used to create this entry
        unsafe { self.value.cast::<H>().as_mut() }
    }
}

impl Drop for ErasedHandler {
    fn drop(&mut self) {
        (self.drop)(self.value);
    }
}

#[cfg(test)]
mod test;
