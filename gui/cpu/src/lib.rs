#![no_std]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod clear;
mod damage;
mod glyph;
mod math;
mod pixel;
mod raster;
pub mod render;
mod strategy;
mod text;

use alloc::{boxed::Box, vec::Vec};
use core::{any::TypeId, hash::Hasher, marker::PhantomData, mem::offset_of, ptr::NonNull};

use blit::{LogicalRect, PhysicalRect, Scale2};
use blit_gui::{
    Context,
    damage::{DamageTracker, tile::TileDamage},
    display_list::{self, Compare, DisplayList},
    image::{ImageData, ImageHandle},
    style::BorderRadius,
    text::system::TextSystem,
};
use blit_util::arena::Arena;

use self::{
    damage::passes::{PassDamage, StageDamage},
    pixel::PixelSpan,
    raster::image::StoredImage,
    render::{ErasedCommand, RenderTarget, Replay},
    strategy::{
        clip::ClipStack,
        command::{Coverage, RasterCommands},
    },
    text::TextRenderer,
};
pub use self::{
    pixel::{Argb8888, Pixel, PixelBuffer, PremultipliedRgbaColor, Rgb8Pixel, Rgba8888, VecBuffer, Xrgb8888},
    render::{DrawContext, Render, RenderContext, Span, Surface},
    strategy::{Direct, RenderStrategy, Scanline},
};

pub struct Renderer<B: PixelBuffer, S: RenderStrategy<B> = Direct, D: DamageTracker = TileDamage> {
    context: RenderState<B>,
    renderers: Vec<ErasedRenderer>,
    paint_clip: u32,
    paint_clips: Vec<u32>,
    strategy: S,
    damage: D,
    pass_damage: PassDamage,
    tile_shift: u32,
}

impl<B: PixelBuffer> Renderer<B> {
    pub fn new(buffer: B, text: TextSystem, config: RendererConfig) -> Self {
        Self {
            context: RenderState {
                buffer,
                scale_factor: 1.0,
                images: Arena::new(),
                text: TextRenderer::new(&config, text),
                list: DisplayList::default(),
                reads_framebuffer: false,
                raster: RasterCommands::default(),
                clips: ClipStack::default(),
                snapshot: Surface {
                    bounds: PhysicalRect::default(),
                    width: 0,
                    height: 0,
                    pixels: Vec::new(),
                    valid: damage::region::Region::default(),
                },
                preserved: Vec::new(),
            },
            renderers: Vec::new(),
            paint_clip: 0,
            paint_clips: Vec::new(),
            strategy: Direct::default(),
            damage: TileDamage::new(config.damage_tile_size),
            pass_damage: PassDamage::default(),
            tile_shift: config.damage_tile_size.trailing_zeros(),
        }
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Renderer<B, S, D> {
    /// registers or replaces a renderer and invalidates retained pixels
    pub fn register<R: 'static>(mut self, renderer: R) -> Self {
        assert!(
            self.context.list.commands().is_empty(),
            "cannot replace a renderer during a frame"
        );
        let kind = TypeId::of::<R>();
        if let Some(value) = self.renderers.iter_mut().find(|value| value.kind == kind) {
            *value = ErasedRenderer::new(renderer);
        } else {
            self.renderers.push(ErasedRenderer::new(renderer));
        }
        self.damage.invalidate();
        self
    }

    /// returns mutable renderer state creating its default when needed and invalidating retained pixels
    pub fn configure<R: Default + 'static>(&mut self) -> &mut R {
        self.damage.invalidate();
        let index = renderer_index::<R>(&mut self.renderers);
        // safety: the lookup selects the entry created for R
        unsafe { self.renderers[index].cast::<R>() }
    }

    /// begins recording drawing inputs for a logical area
    /// bounds start as the physical area and may be widened for outsets
    pub fn record(&mut self, area: LogicalRect) -> RecordContext<'_, B::Pixel> {
        let context = &mut self.context;
        RecordContext {
            scale: context.scale_factor,
            area,
            bounds: area.to_physical(Scale2::uniform(context.scale_factor)),
            clip: self.paint_clip,
            renderers: &mut self.renderers,
            marker: PhantomData,
            list: &mut context.list,
            reads_framebuffer: &mut context.reads_framebuffer,
        }
    }

    pub fn strategy<T: RenderStrategy<B>>(self, strategy: T) -> Renderer<B, T, D> {
        Renderer {
            context: self.context,
            renderers: self.renderers,
            paint_clip: self.paint_clip,
            paint_clips: self.paint_clips,
            strategy,
            damage: self.damage,
            pass_damage: self.pass_damage,
            tile_shift: self.tile_shift,
        }
    }

    pub fn damage_tracker<T: DamageTracker>(self, mut damage: T) -> Renderer<B, S, T> {
        damage.invalidate();
        Renderer {
            context: self.context,
            renderers: self.renderers,
            paint_clip: self.paint_clip,
            paint_clips: self.paint_clips,
            strategy: self.strategy,
            damage,
            pass_damage: self.pass_damage,
            tile_shift: self.tile_shift,
        }
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        let id = self.context.images.insert_with(|id| StoredImage::new(id.into(), data));
        self.context.images.get(id).unwrap().handle.clone()
    }

    /// buffer age counts presentations since this buffer was last used
    /// zero means its contents are unknown and forces a full repaint
    pub fn render(&mut self, buffer_age: u8) {
        let screen = self.screen();
        let scale = self.context.scale_factor;
        self.context.raster.scale = scale;
        self.context.raster.screen = PhysicalRect {
            x: self.context.buffer.x_offset() as i32,
            ..screen
        };
        self.damage.update(&self.context.list, screen, scale, buffer_age);
        if !self.damage.is_empty() {
            let context = &mut self.context;
            for (index, command) in context.list.commands().iter().enumerate() {
                // todo: restrict effect frame dispatch to damage and required source regions
                if context.reads_framebuffer || self.damage.intersects(command.bounds, command.clip) {
                    // entries pushed by this dispatch inherit the index as their source
                    context.raster.source = index as u32;
                    let mut draw = DrawContext {
                        scale,
                        area: command.area,
                        bounds: command.bounds,
                        clip: command.clip,
                        offset: 0,
                        screen,
                        clips: &context.clips,
                        images: &context.images,
                        list: &context.list,
                        commands: &mut context.raster,
                    };
                    context.list.replay::<Replay<B::Pixel>>(index)(index, &mut draw);
                }
            }
        }
        self.pass_damage
            .update(&self.context, &self.damage, screen, self.tile_shift);
        let plan = &mut self.pass_damage;
        let mut damage = StageDamage {
            tracker: &mut self.damage,
            region: plan.stages.first(),
        };
        let context = &mut self.context;
        if !damage.is_empty() {
            let end = context.raster.len();
            if let Some(region) = plan.stages.first() {
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
                        context
                            .buffer
                            .write_line(y as usize, bounds.x as usize..(bounds.x + bounds.width) as usize)
                            .fill(B::Pixel::background());
                    }
                }
            }
            let mut start = 0;
            for (stage, node) in plan.passes.iter().enumerate() {
                context.raster.range = start..node.index;
                context.raster.cull(
                    &context.list,
                    &context.clips,
                    &damage,
                    screen,
                    self.tile_shift,
                    &mut plan.ranges,
                );
                self.strategy.render(context, &damage);
                start = node.index + 1;
                damage.region = Some(&plan.stages[stage + 1]);
                let clip = context.raster.clip(node.index);
                if damage.intersects(node.bounds, clip) {
                    let snapshot = &mut context.snapshot;
                    snapshot.width = context.buffer.width();
                    snapshot.height = context.buffer.height();
                    snapshot.bounds = node
                        .reads
                        .rects()
                        .reduce(|bounds, next| bounds.union(next))
                        .unwrap_or_default();
                    let len = snapshot.bounds.width as usize * snapshot.bounds.height as usize;
                    if snapshot.pixels.len() < len {
                        snapshot.pixels.resize(len, B::Pixel::background());
                    }
                    snapshot.valid.clone_from(&node.reads);
                    for bounds in node.reads.rects() {
                        for y in bounds.y..bounds.y + bounds.height {
                            let range = bounds.x as usize..(bounds.x + bounds.width) as usize;
                            let start = (y - snapshot.bounds.y) as usize * snapshot.bounds.width as usize
                                + (bounds.x - snapshot.bounds.x) as usize;
                            snapshot.pixels[start..start + range.len()]
                                .copy_from_slice(context.buffer.read_line(y as usize, range));
                        }
                    }
                    let bounds = context.raster.bounds(node.index);
                    let area =
                        context.list.commands()[context.raster.metadata[node.index as usize].source as usize].area;
                    context.raster.entries[node.index as usize].render(
                        &mut context.raster.prepared,
                        &mut RenderContext {
                            scale,
                            area,
                            bounds,
                            clips: &context.clips,
                            clip,
                            screen: context.raster.screen,
                            damage_bounds: node.bounds,
                            list: &context.list,
                            text: &mut context.text,
                            gradients: &mut context.raster.gradients,
                            images: &mut context.images,
                            source: Some(snapshot),
                            scratch: &context.raster.scratch,
                            target: RenderTarget::Pass {
                                target: &mut context.buffer,
                                output: &plan.stages[stage + 1],
                            },
                        },
                    );
                }
            }
            context.raster.range = start..end;
            context.raster.cull(
                &context.list,
                &context.clips,
                &damage,
                screen,
                self.tile_shift,
                &mut plan.ranges,
            );
            self.strategy.render(context, &damage);
            if !plan.passes.is_empty() {
                let mut offset = 0;
                for bounds in plan.preserve.rects() {
                    for y in bounds.y..bounds.y + bounds.height {
                        let pixels = context
                            .buffer
                            .write_line(y as usize, bounds.x as usize..(bounds.x + bounds.width) as usize);
                        pixels.copy_from_slice(&context.preserved[offset..offset + pixels.len()]);
                        offset += pixels.len();
                    }
                }
            }
        }
        context.raster.clear();
        // taken so a panicking tracker cannot leave a swapped in list behind
        let mut list = core::mem::take(&mut context.list);
        self.damage.finish_frame(&mut list);
        context.list = list;
        for renderer in &mut self.renderers {
            (renderer.finish)(renderer);
        }
        self.reset_frame();
    }

    /// disjoint dirty regions written by the last render
    pub fn damage(&mut self) -> impl Iterator<Item = PhysicalRect> + '_ {
        let screen = self.screen();
        let mut present = self.pass_damage.stages.last().map(damage::region::Region::rects);
        let damage = &self.damage;
        let ranges = &mut self.pass_damage.ranges;
        let mut line = 0;
        let mut end = 0;
        let mut index = 0;
        ranges.clear();
        core::iter::from_fn(move || {
            if let Some(rects) = &mut present {
                return rects.next();
            }
            loop {
                if index < ranges.len() {
                    let range = &ranges[index];
                    index += 1;
                    return Some(PhysicalRect::new(
                        range.start as i32,
                        line,
                        (range.end - range.start) as i32,
                        end - line,
                    ));
                }
                line = end;
                if line >= screen.height {
                    return None;
                }
                end = damage.line_ranges(line, screen, ranges);
                index = 0;
            }
        })
    }

    /// clears pending inputs before a new frame
    pub fn reset_frame(&mut self) {
        let context = &mut self.context;
        context.list.clear();
        context.reads_framebuffer = false;
        context.clips.clear();
        context.text.finish_frame();
        context.images.retain(|image| !image.handle.is_uniquely_owned());
        self.paint_clip = 0;
        self.paint_clips.clear();
    }

    pub fn screen(&self) -> PhysicalRect {
        PhysicalRect::new(
            0,
            0,
            i32::try_from(self.context.buffer.width()).unwrap_or(0),
            i32::try_from(self.context.buffer.height()).unwrap_or(0),
        )
    }

    pub fn buffer(&self) -> &B {
        &self.context.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut B {
        &mut self.context.buffer
    }

    /// forces a full redraw on the next render
    pub fn invalidate_all(&mut self) {
        self.damage.invalidate();
    }

    pub fn set_scale(&mut self, scale: f32) {
        assert!(scale.is_finite() && scale > 0.0);
        if self.context.scale_factor != scale {
            self.context.scale_factor = scale;
            self.invalidate_all();
        }
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> blit::Context for Renderer<B, S, D> {
    type Scalar = f32;

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

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> blit::HasFrameStore for Renderer<B, S, D> {
    fn store(&self) -> &blit::FrameStore {
        self.context.list.store()
    }

    fn store_mut(&mut self) -> blit::FrameStoreMut<'_> {
        self.context.list.store_mut()
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Context for Renderer<B, S, D> {
    fn text(&mut self) -> &mut TextSystem {
        &mut self.context.text.system
    }
}

pub struct RecordContext<'a, P: Pixel> {
    pub scale: f32,
    pub area: LogicalRect,
    pub bounds: PhysicalRect,
    clip: u32,
    renderers: &'a mut Vec<crate::ErasedRenderer>,
    marker: PhantomData<fn(P) -> P>,
    list: &'a mut DisplayList,
    reads_framebuffer: &'a mut bool,
}

impl<P: Pixel> RecordContext<'_, P> {
    /// records an input and the function that draws it
    /// damage tracking compares the input and the callback type
    /// put changing values in the input rather than capturing them in the callback
    #[inline]
    pub fn push<T, F>(&mut self, value: T, draw: F)
    where
        T: Compare,
        F: Fn(&T, &mut DrawContext<'_, P>) + 'static,
    {
        fn replay<P, T, F>(index: usize, cx: &mut DrawContext<'_, P>)
        where
            P: Pixel,
            T: Compare,
            F: Fn(&T, &mut DrawContext<'_, P>) + 'static,
        {
            let (recorded, offset) = cx.list.input::<Recorded<T, F>>(index);
            cx.offset = offset + offset_of!(Recorded<T, F>, value) as u32;
            (recorded.draw)(&recorded.value, cx);
        }

        self.list.push(
            self.area,
            self.bounds,
            self.clip,
            Recorded { value, draw },
            replay::<P, T, F> as Replay<P>,
        );
    }

    pub fn store(&self) -> &blit::FrameStore {
        self.list.store()
    }

    pub fn store_mut(&mut self) -> blit::FrameStoreMut<'_> {
        self.list.store_mut()
    }

    /// prepares once on the first visible span and reuses the result for the frame
    /// opaque guarantees fully opaque output inside a physical rectangle intersected with bounds and clip
    pub fn push_render<R>(&mut self, value: R::Input, opaque: Option<PhysicalRect>)
    where
        R: Render<P> + Default,
    {
        let renderer = self.renderer::<R>();
        let input = RenderInput {
            value,
            opaque: opaque.unwrap_or_default(),
        };
        self.push(input, move |input, cx| {
            let opaque = input.opaque.intersection(cx.bounds).is_some();
            let call = ErasedCommand {
                renderer,
                value: cx.offset + offset_of!(RenderInput<R::Input>, value) as u32,
                opaque: if opaque {
                    cx.offset + offset_of!(RenderInput<R::Input>, opaque) as u32
                } else {
                    0
                },
                reads: None,
                prepare: Some(|renderer, offset, prepared, cx| {
                    // safety: recording pairs this offset with R::Input and the renderer with R
                    let value = unsafe { cx.list.store().get::<R::Input>(offset) };
                    let renderer = unsafe { renderer.cast::<R>().as_mut() };
                    prepared.push(renderer.prepare(value, cx))
                }),
                render: Some(|renderer, offset, prepared, cx| {
                    // safety: recording pairs this value and renderer with R
                    let value = unsafe { prepared.get::<R::Prepared>(offset) };
                    unsafe { renderer.cast::<R>().as_mut() }.render(value, cx);
                }),
            };
            let metadata = cx.commands.push(call, cx.bounds, cx.clip);
            if opaque {
                metadata.coverage = Coverage::Rect;
            }
        });
    }

    /// records a framebuffer effect with its source footprint for each output region
    pub fn push_pass<R>(&mut self, value: R::Input)
    where
        R: Render<P, Prepared = <R as Render<P>>::Input> + Default,
    {
        let renderer = self.renderer::<R>();
        *self.reads_framebuffer = true;
        self.push(value, move |_, cx| {
            let call = ErasedCommand {
                renderer,
                value: cx.offset,
                opaque: 0,
                prepare: None,
                render: Some(|renderer, offset, _, cx| {
                    // safety: recording pairs this value and renderer with R
                    let value = unsafe { cx.list.store().get::<R::Input>(offset) };
                    unsafe { renderer.cast::<R>().as_mut() }.render(value, cx);
                }),
                reads: Some(|renderer, offset, list, output| {
                    // safety: recording pairs this offset with R::Input
                    let value = unsafe { list.store().get::<R::Input>(offset) };
                    // safety: recording selects the renderer for R
                    unsafe { renderer.cast::<R>().as_ref() }.reads(value, output)
                }),
            };
            cx.commands.push(call, cx.bounds, cx.clip);
        });
    }

    fn renderer<R>(&mut self) -> NonNull<()>
    where
        R: Render<P> + Default,
    {
        let index = crate::renderer_index::<R>(self.renderers);
        let renderer = &mut self.renderers[index];
        renderer.finish = |renderer| {
            // safety: recording selects the entry created for R
            unsafe { renderer.cast::<R>() }.finish_frame();
        };
        renderer.value
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RendererConfig {
    /// width and height of damage and coverage cells in physical pixels as a nonzero power of two
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
pub struct RenderState<B: PixelBuffer> {
    buffer: B,
    scale_factor: f32,
    images: Arena<StoredImage>,
    text: TextRenderer,
    list: DisplayList,
    reads_framebuffer: bool,
    raster: RasterCommands<B::Pixel>,
    clips: ClipStack,
    snapshot: Surface<B::Pixel>,
    preserved: Vec<B::Pixel>,
}

struct Recorded<T, F> {
    value: T,
    draw: F,
}

impl<T: Compare, F: 'static> Compare for Recorded<T, F> {
    fn equal(a: &Self, left: &DisplayList, b: &Self, right: &DisplayList) -> bool {
        T::equal(&a.value, left, &b.value, right)
    }

    fn hash(input: &Self, list: &DisplayList, state: &mut impl Hasher) {
        T::hash(&input.value, list, state);
    }
}

struct RenderInput<T> {
    value: T,
    opaque: PhysicalRect,
}

impl<T: Compare> Compare for RenderInput<T> {
    fn equal(a: &Self, left: &DisplayList, b: &Self, right: &DisplayList) -> bool {
        T::equal(&a.value, left, &b.value, right)
    }

    fn hash(input: &Self, list: &DisplayList, state: &mut impl Hasher) {
        T::hash(&input.value, list, state);
    }
}

struct ErasedRenderer {
    kind: TypeId,
    value: NonNull<()>,
    drop: fn(NonNull<()>),
    finish: fn(&mut Self),
}

impl ErasedRenderer {
    fn new<R: 'static>(renderer: R) -> Self {
        Self {
            kind: TypeId::of::<R>(),
            value: NonNull::from(Box::leak(Box::new(renderer))).cast(),
            finish: |_| {},
            drop: |value| {
                // safety: new transfers ownership of one boxed R to this entry
                unsafe { drop(Box::from_raw(value.cast::<R>().as_ptr())) };
            },
        }
    }

    unsafe fn cast<R>(&mut self) -> &mut R {
        // safety: callers select the type used to create this entry
        unsafe { self.value.cast::<R>().as_mut() }
    }
}

impl Drop for ErasedRenderer {
    fn drop(&mut self) {
        (self.drop)(self.value);
    }
}

fn renderer_index<R>(renderers: &mut Vec<ErasedRenderer>) -> usize
where
    R: Default + 'static,
{
    let kind = TypeId::of::<R>();
    if let Some(index) = renderers.iter().position(|value| value.kind == kind) {
        index
    } else {
        let index = renderers.len();
        renderers.push(ErasedRenderer::new(R::default()));
        index
    }
}

#[cfg(test)]
mod test;
