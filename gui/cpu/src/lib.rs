pub mod arena;
pub mod damage;
mod glyph;
mod paint;
mod pixel;
pub mod raster;
mod render;
pub mod shadow;
mod strategy;
mod text;

use std::marker::PhantomData;

use blit::{LogicalRect, Paint, PhysicalRect, Scale2};
use blit_arena::Arena;
use blit_gui::{
    Context, TextSystem,
    image::{ImageData, ImageFormat, ImageHandle},
    style::BorderRadius,
};

use self::{
    damage::{
        DamageInput,
        passes::{PassDamage, StageDamage},
    },
    paint::{Draws, ReadRequest},
    pixel::PixelSpan,
    render::image_patch::AlphaRows,
    strategy::{
        clip::ClipStack,
        command::{CommandList, Payload},
    },
    text::TextRenderer,
};
pub use self::{
    damage::{DamageTracker, tile::TileDamage},
    paint::{DrawContext, Handler, Surface},
    pixel::{Argb8888, Pixel, PixelBuffer, PremultipliedRgbaColor, Rgb8Pixel, Rgba8888, VecBuffer, Xrgb8888},
    strategy::{Direct, RenderStrategy, Scanline},
};

pub struct Renderer<B: PixelBuffer, S: RenderStrategy<B> = Direct, D: DamageTracker = TileDamage, H: 'static = Builtins>
{
    context: RenderContext<B>,
    damage_input: DamageInput,
    text_system: TextSystem,
    paint_clip: u32,
    paint_clips: Vec<u32>,
    strategy: S,
    damage: D,
    pass_damage: PassDamage,
    pass_shift: u32,
    handlers: PhantomData<fn() -> H>,
}

impl<B: PixelBuffer> Renderer<B> {
    pub fn new(buffer: B, text: TextSystem, config: RendererConfig) -> Self {
        Self {
            context: RenderContext {
                buffer,
                scale_factor: 1.0,
                images: Arena::new(),
                text: TextRenderer::new(&config),
                commands: CommandList::default(),
                clips: ClipStack::default(),
                draws: Draws::new(Builtins::default()),
            },
            damage_input: DamageInput::default(),
            text_system: text,
            paint_clip: 0,
            paint_clips: Vec::new(),
            strategy: Direct::default(),
            damage: TileDamage::new(config.damage_tile_size),
            pass_damage: PassDamage::default(),
            pass_shift: config.damage_tile_size.trailing_zeros(),
            handlers: PhantomData,
        }
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker, H: 'static> Renderer<B, S, D, H> {
    /// selects the handlers available through the paint trait
    pub fn handlers<T: 'static>(mut self, handlers: T) -> Renderer<B, S, D, T> {
        assert!(
            self.damage_input.records.is_empty(),
            "cannot replace handlers during a frame"
        );
        self.context.draws.handlers = Box::new(handlers);
        self.damage.invalidate();
        Renderer {
            context: self.context,
            damage_input: self.damage_input,
            text_system: self.text_system,
            paint_clip: self.paint_clip,
            paint_clips: self.paint_clips,
            strategy: self.strategy,
            damage: self.damage,
            pass_damage: self.pass_damage,
            pass_shift: self.pass_shift,
            handlers: PhantomData,
        }
    }

    pub fn handlers_mut(&mut self) -> &mut H {
        self.damage.invalidate();
        self.context.draws.handlers.downcast_mut().unwrap()
    }

    pub fn strategy<T: RenderStrategy<B>>(self, strategy: T) -> Renderer<B, T, D, H> {
        Renderer {
            context: self.context,
            damage_input: self.damage_input,
            text_system: self.text_system,
            paint_clip: self.paint_clip,
            paint_clips: self.paint_clips,
            strategy,
            damage: self.damage,
            pass_damage: self.pass_damage,
            pass_shift: self.pass_shift,
            handlers: PhantomData,
        }
    }

    pub fn damage_tracker<T: DamageTracker>(self, mut damage: T) -> Renderer<B, S, T, H> {
        damage.invalidate();
        Renderer {
            context: self.context,
            damage_input: self.damage_input,
            text_system: self.text_system,
            paint_clip: self.paint_clip,
            paint_clips: self.paint_clips,
            strategy: self.strategy,
            damage,
            pass_damage: self.pass_damage,
            pass_shift: self.pass_shift,
            handlers: PhantomData,
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
        self.damage.update(&self.damage_input, screen, scale, buffer_age);
        self.pass_damage.update(
            &self.damage_input,
            &self.context.commands,
            &self.context.draws,
            &self.damage,
            screen,
            scale,
            self.pass_shift,
        );
        let plan = &self.pass_damage;
        let mut damage = StageDamage {
            tracker: &mut self.damage,
            region: plan.stages.first(),
        };
        let context = &mut self.context;
        if !damage.is_empty() {
            if let Some(region) = plan.stages.first() {
                context
                    .draws
                    .read(&mut context.buffer, ReadRequest::Preserve(&plan.preserve));
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
                context.commands.range = Some(start..node.index);
                self.strategy.render(context, &damage);
                start = node.index + 1;
                damage.region = Some(&plan.stages[stage + 1]);
                let Payload::Pass(id) = context.commands.get(node.index) else {
                    unreachable!()
                };
                if damage.intersects(node.bounds, context.commands.clip(node.index)) {
                    context
                        .draws
                        .read(&mut context.buffer, ReadRequest::Snapshot(&node.reads));
                    context.draws.pass(
                        id,
                        &mut context.buffer,
                        &context.clips,
                        node.bounds,
                        context.commands.clip(node.index),
                        &context.images,
                        &plan.stages[stage + 1],
                    );
                }
            }
            context.commands.range = Some(start..context.commands.len());
            self.strategy.render(context, &damage);
            if !plan.passes.is_empty() {
                context.draws.restore(&mut context.buffer, &plan.preserve);
            }
        }
        context.commands.clear();
        context.clips.clear();
        context.draws.clear();
        context.text.finish_frame(&mut self.text_system);
        context.images.retain(|image| !image.handle.is_uniquely_owned());
        self.damage.finish_frame(&mut self.damage_input);
        self.damage_input.clear();
        self.paint_clip = 0;
        self.text_system.finish_frame();
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
            self.damage_input.records.is_empty(),
            "cannot change scale during a frame"
        );
        if self.context.scale_factor != scale {
            self.context.scale_factor = scale;
            self.invalidate_all();
        }
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker, H: 'static> blit::Context for Renderer<B, S, D, H> {
    type Scalar = f32;
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker, H: Handler<T, B::Pixel>, T> Paint<T>
    for Renderer<B, S, D, H>
{
    fn paint(&mut self, bounds: LogicalRect, command: T) {
        let context = &mut self.context;
        let handlers = context.draws.handlers.downcast_mut::<H>().unwrap();
        handlers.paint(
            command,
            &mut PaintContext {
                scale: context.scale_factor,
                bounds: bounds.to_physical(Scale2::uniform(context.scale_factor)),
                clip: self.paint_clip,
                damage_input: &mut self.damage_input,
                data: context.draws.data.writer(),
                prepared: &mut context.draws.prepared,
                commands: &mut context.commands,
                images: &mut context.images,
                text: &mut context.text,
                text_system: &mut self.text_system,
                handlers: PhantomData,
            },
        );
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker, H: 'static> Context for Renderer<B, S, D, H> {
    fn text(&mut self) -> &mut TextSystem {
        &mut self.text_system
    }

    fn push_clip(&mut self, area: LogicalRect, radius: BorderRadius) {
        self.paint_clips.push(self.paint_clip);
        self.damage_input.clips.push(damage::Clip {
            parent: self.paint_clip,
            area,
            radius,
        });
        self.context
            .clips
            .push_node(self.paint_clip, area, radius, self.context.scale_factor);
        self.paint_clip = u32::try_from(self.damage_input.clips.len()).expect("too many paint clips");
    }

    fn pop_clip(&mut self) {
        self.paint_clip = self.paint_clips.pop().expect("no paint clip to pop");
    }
}

pub struct PaintContext<'a, H, P: Pixel> {
    pub scale: f32,
    pub bounds: PhysicalRect,
    pub data: arena::Writer<'a>,
    clip: u32,
    damage_input: &'a mut DamageInput,
    prepared: &'a mut Vec<paint::Prepared<P>>,
    commands: &'a mut CommandList,
    images: &'a mut Arena<StoredImage>,
    text: &'a mut TextRenderer,
    text_system: &'a mut TextSystem,
    handlers: PhantomData<fn() -> H>,
}

pub struct Span<'a, P> {
    pub x: i32,
    pub y: i32,
    pub pixels: &'a mut [P],
    /// clip coverage to apply when blending pixels
    pub coverage: u8,
    pub data: &'a arena::Arena,
    images: &'a Arena<StoredImage>,
}

#[derive(Default)]
pub struct Builtins {
    pub shadow: shadow::Renderer,
}

impl AsMut<Builtins> for Builtins {
    fn as_mut(&mut self) -> &mut Self {
        self
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
    commands: CommandList,
    clips: ClipStack,
    draws: Draws<B::Pixel>,
}

struct StoredImage {
    handle: ImageHandle,
    data: ImageData,
    alpha_rows: AlphaRows,
    has_opaque_spans: bool,
    opaque: bool,
}

impl StoredImage {
    fn new(handle: ImageHandle, data: ImageData) -> Self {
        let width = data.texture_rect.width as usize;
        let height = data.texture_rect.height as usize;
        let bytes = data.pixels.bytes();
        let mut has_opaque_spans = false;
        let rgba_opaque = || {
            (0..height).all(|line| {
                bytes[line * data.stride_bytes..][..width * 4]
                    .chunks_exact(4)
                    .all(|pixel| pixel[3] == 255)
            })
        };
        let (alpha_rows, opaque) = match data.format {
            ImageFormat::Rgb8 | ImageFormat::Luma8 => (AlphaRows::default(), true),
            ImageFormat::Rgba8 => (AlphaRows::default(), rgba_opaque()),
            ImageFormat::Rgba8Premultiplied if rgba_opaque() => (AlphaRows::default(), true),
            ImageFormat::Rgba8Premultiplied if width > u16::MAX as usize => (AlphaRows::default(), false),
            ImageFormat::Rgba8Premultiplied => {
                let mut rows = Vec::with_capacity(height * 4);
                for y in 0..height {
                    let row = &bytes[y * data.stride_bytes..][..width * 4];
                    let mut visible_start = width;
                    let mut visible_end = 0;
                    let mut run_start = 0;
                    let mut opaque_start = 0;
                    let mut opaque_end = 0;
                    for (x, alpha) in row.chunks_exact(4).map(|pixel| pixel[3]).chain([0]).enumerate() {
                        if alpha != 0 {
                            visible_start = visible_start.min(x);
                            visible_end = x + 1;
                        }
                        if alpha == 255 {
                            continue;
                        }
                        if x - run_start > opaque_end - opaque_start {
                            opaque_start = run_start;
                            opaque_end = x;
                        }
                        run_start = x + 1;
                    }
                    visible_start = visible_start.min(visible_end);
                    has_opaque_spans |= opaque_start < opaque_end;
                    rows.extend([
                        visible_start as u16,
                        visible_end as u16,
                        opaque_start as u16,
                        opaque_end as u16,
                    ]);
                }
                (AlphaRows(rows.into_boxed_slice()), false)
            }
            ImageFormat::Alpha8(_)
                if (0..height).all(|line| {
                    bytes[line * data.stride_bytes..][..width]
                        .iter()
                        .all(|alpha| *alpha == 255)
                }) =>
            {
                (AlphaRows::default(), true)
            }
            ImageFormat::Alpha8(_) if width > u16::MAX as usize => (AlphaRows::default(), false),
            ImageFormat::Alpha8(_) => {
                let mut rows = Vec::with_capacity(height * 2);
                for y in 0..height {
                    let row = &bytes[y * data.stride_bytes..][..width];
                    let mut visible_start = width;
                    let mut visible_end = 0;
                    for (x, alpha) in row.iter().enumerate() {
                        if *alpha != 0 {
                            visible_start = visible_start.min(x);
                            visible_end = x + 1;
                        }
                    }
                    visible_start = visible_start.min(visible_end);
                    rows.extend([visible_start as u16, visible_end as u16]);
                }
                (AlphaRows(rows.into_boxed_slice()), false)
            }
        };
        Self {
            handle,
            data,
            alpha_rows,
            has_opaque_spans,
            opaque,
        }
    }
}

#[cfg(test)]
mod test;
