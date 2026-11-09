pub mod damage;
mod glyph;
mod paint;
mod pixel;
pub mod raster;
mod render;
pub mod shadow;
mod strategy;
mod text;

use blit::{LogicalRect, PhysicalRect};
use blit_arena::Arena;
use blit_gui::{
    Context, TextSystem,
    display_list::{self, DisplayList},
    image::{ImageData, ImageFormat, ImageHandle},
    style::BorderRadius,
};

use self::{
    damage::passes::{PassDamage, StageDamage},
    paint::{DrawTarget, ErasedHandler},
    pixel::PixelSpan,
    render::image_patch::AlphaRows,
    strategy::{clip::ClipStack, command::RasterCommands},
    text::TextRenderer,
};
pub use self::{
    damage::{DamageTracker, tile::TileDamage},
    paint::{Handler, PaintContext},
    pixel::{Argb8888, Pixel, PixelBuffer, PremultipliedRgbaColor, Rgb8Pixel, Rgba8888, VecBuffer, Xrgb8888},
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
            for command in context.list.commands() {
                (command.backend().prepare)(
                    command,
                    context.list.data(),
                    scale,
                    &context.images,
                    std::ptr::NonNull::from(&mut context.raster).cast(),
                );
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
                            data: context.list.data(),
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

pub struct Span<'a, P> {
    pub x: i32,
    pub y: i32,
    pub pixels: &'a mut [P],
    /// clip coverage to apply when blending pixels
    pub coverage: u8,
    pub data: &'a blit_gui::arena::Arena,
    images: &'a Arena<StoredImage>,
}

pub struct DrawContext<'a, P> {
    /// physical drawing bounds independent of the current span
    pub bounds: PhysicalRect,
    data: &'a blit_gui::arena::Arena,
    text: &'a mut TextRenderer,
    gradients: &'a mut render::fill::Gradients,
    images: &'a mut Arena<StoredImage>,
    source: Option<&'a Surface<P>>,
    target: DrawTarget<'a, P>,
}

pub struct Surface<P> {
    width: usize,
    height: usize,
    pixels: Vec<P>,
    valid: damage::region::Region,
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
    list: DisplayList<raster::Operations>,
    raster: RasterCommands<B::Pixel>,
    clips: ClipStack,
    snapshot: Surface<B::Pixel>,
    preserved: Vec<B::Pixel>,
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
