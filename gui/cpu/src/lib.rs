mod clear;
mod damage;
mod glyph;
mod pixel;
mod raster;
pub mod render;
mod strategy;
mod text;

use std::{any::TypeId, hash::Hasher, marker::PhantomData, mem::offset_of, ptr::NonNull};

use blit::{FrameSlice, LogicalRect, PhysicalRect, Scale2, internal::bump::Offset};
use blit_arena::Arena;
use blit_gui::{
    Context, TextSystem,
    damage::{DamageTracker, tile::TileDamage},
    display_list::{self, Command, Compare, DisplayList},
    image::{ImageData, ImageHandle},
    style::BorderRadius,
};

use self::{
    damage::passes::{PassDamage, StageDamage},
    pixel::PixelSpan,
    raster::image::StoredImage,
    render::{DispatchContext, ErasedCommand, RenderTarget},
    strategy::{
        clip::ClipStack,
        command::{Coverage, RasterCommands},
    },
    text::TextRenderer,
};
pub use self::{
    pixel::{Argb8888, Pixel, PixelBuffer, PremultipliedRgbaColor, Rgb8Pixel, Rgba8888, VecBuffer, Xrgb8888},
    render::{Render, RenderContext, Span, Surface},
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
        assert!(
            self.context.list.commands().is_empty(),
            "cannot configure a renderer during a frame"
        );
        self.damage.invalidate();
        let index = renderer_index::<R>(&mut self.renderers);
        // safety: the lookup selects the entry created for R
        unsafe { self.renderers[index].cast::<R>() }
    }

    /// begins recording drawing inputs within bounds
    pub fn record(&mut self, bounds: LogicalRect) -> RecordContext<'_, B::Pixel> {
        let context = &mut self.context;
        RecordContext {
            scale: context.scale_factor,
            bounds: bounds.to_physical(Scale2::uniform(context.scale_factor)),
            clip: self.paint_clip,
            renderers: &mut self.renderers,
            marker: PhantomData,
            list: &mut context.list,
            reads_framebuffer: &mut context.reads_framebuffer,
            images: &mut context.images,
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
        assert!(self.paint_clips.is_empty(), "unbalanced paint clips");
        let screen = self.screen();
        let scale = self.context.scale_factor;
        self.context.raster.scale = scale;
        self.damage.update(&self.context.list, screen, scale, buffer_age);
        if !self.damage.is_empty() {
            let context = &mut self.context;
            let mut dispatch = render::DispatchContext {
                screen,
                clips: &context.clips,
                images: &context.images,
                commands: &mut context.raster,
            };
            for command in context.list.commands() {
                // todo: restrict effect frame dispatch to damage and required source regions
                if context.reads_framebuffer || self.damage.intersects(command.bounds, command.clip) {
                    // safety: recording pairs dispatch with this renderer's dispatch context
                    unsafe {
                        command.dispatch(&context.list, std::ptr::NonNull::from(&mut dispatch).cast());
                    }
                }
            }
        }
        self.pass_damage
            .update(&self.context, &self.damage, screen, self.tile_shift);
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
                context
                    .raster
                    .cull(&context.list, &context.clips, &damage, screen, self.tile_shift);
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
                    context.raster.entries[node.index as usize].render(
                        &mut context.raster.prepared,
                        &mut RenderContext {
                            scale,
                            bounds,
                            list: &context.list,
                            text: &mut context.text,
                            gradients: &mut context.raster.gradients,
                            images: &mut context.images,
                            source: Some(snapshot),
                            scratch: &context.raster.scratch,
                            target: RenderTarget::Pass {
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
            context
                .raster
                .cull(&context.list, &context.clips, &damage, screen, self.tile_shift);
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
        context.reads_framebuffer = false;
        context.clips.clear();
        // taken so a panicking tracker cannot leave a swapped in list behind
        let mut list = std::mem::take(&mut context.list);
        self.damage.finish_frame(&mut list);
        list.clear();
        context.list = list;
        context.text.finish_frame();
        for renderer in &mut self.renderers {
            (renderer.finish)(renderer);
        }
        context.images.retain(|image| !image.handle.is_uniquely_owned());
        self.paint_clip = 0;
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

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> blit::FrameStore for Renderer<B, S, D> {
    fn frame_data(&self) -> &blit::FrameData {
        self.context.list.inputs()
    }

    fn store_slice<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> FrameSlice<T> {
        self.context.list.store_slice(values)
    }
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

pub struct RecordContext<'a, P: Pixel> {
    pub scale: f32,
    pub bounds: PhysicalRect,
    clip: u32,
    renderers: &'a mut Vec<crate::ErasedRenderer>,
    marker: PhantomData<fn(P) -> P>,
    list: &'a mut DisplayList,
    reads_framebuffer: &'a mut bool,
    images: &'a mut blit_arena::Arena<StoredImage>,
}

impl<P: Pixel> RecordContext<'_, P> {
    pub fn store_slice<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> FrameSlice<T> {
        self.list.store_slice(values)
    }

    /// records an already prepared value without running preparation
    /// opaque guarantees fully opaque output inside a physical rectangle intersected with bounds and clip
    #[inline]
    pub fn push_ready<R>(&mut self, value: R::Input, opaque: Option<PhysicalRect>)
    where
        R: Render<P, Prepared = <R as Render<P>>::Input> + Default,
    {
        let renderer = self.renderer::<R>();
        self.record_ready::<R, false>(value, opaque, renderer);
    }

    /// prepares once on the first visible span and reuses the result for the frame
    /// opaque guarantees fully opaque output inside a physical rectangle intersected with bounds and clip
    pub fn push<R>(&mut self, value: R::Input, opaque: Option<PhysicalRect>)
    where
        R: Render<P> + Default,
    {
        let renderer = self.renderer::<R>();
        self.record(value, opaque.unwrap_or_default(), move |offset, command, target| {
            let call = ErasedCommand {
                renderer,
                value: offset,
                opaque: Offset::new(0),
                reads: None,
                prepare: Some(|renderer, offset, prepared, cx| {
                    // safety: recording pairs this offset with R::Input and the renderer with R
                    let value = unsafe { cx.list.data().get(Offset::<R::Input>::new(offset)) };
                    let renderer = unsafe { renderer.cast::<R>().as_mut() };
                    prepared.push(renderer.prepare(value, cx)).byte()
                }),
                render: Some(|renderer, offset, prepared, cx| {
                    // safety: recording pairs this value and renderer with R
                    let value = unsafe { prepared.get(Offset::<R::Prepared>::new(offset)) };
                    unsafe { renderer.cast::<R>().as_mut() }.render(value, cx);
                }),
            };
            target.push(call, command.bounds, command.clip);
        });
    }

    /// records a framebuffer effect with its source footprint for each output region
    pub fn push_pass<R>(&mut self, value: R::Input)
    where
        R: Render<P, Prepared = <R as Render<P>>::Input> + Default,
    {
        let renderer = self.renderer::<R>();
        *self.reads_framebuffer = true;
        self.record_ready::<R, true>(value, None, renderer);
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        let id = self.images.insert_with(|id| StoredImage::new(id.into(), data));
        self.images.get(id).unwrap().handle.clone()
    }

    fn record_ready<R, const READS: bool>(
        &mut self,
        value: R::Input,
        opaque: Option<PhysicalRect>,
        renderer: NonNull<()>,
    ) where
        R: Render<P, Prepared = <R as Render<P>>::Input>,
    {
        self.record(value, opaque.unwrap_or_default(), move |offset, command, target| {
            let call = ErasedCommand {
                renderer,
                value: offset,
                opaque: Offset::new(0),
                prepare: None,
                render: Some(|renderer, offset, _, cx| {
                    // safety: recording pairs this value and renderer with R
                    let value = unsafe { cx.list.data().get(Offset::<R::Input>::new(offset)) };
                    unsafe { renderer.cast::<R>().as_mut() }.render(value, cx);
                }),
                reads: READS.then_some({
                    (|renderer: NonNull<()>, offset, list: &DisplayList, output| {
                        // safety: recording pairs this offset with R::Input
                        let value = unsafe { list.data().get(Offset::<R::Input>::new(offset)) };
                        // safety: recording selects the renderer for R
                        unsafe { renderer.cast::<R>().as_ref() }.reads(value, output)
                    }) as _
                }),
            };
            target.push(call, command.bounds, command.clip);
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

    fn record<T, F>(&mut self, value: T, opaque: PhysicalRect, prepare: F)
    where
        T: Compare,
        F: Fn(u32, &Command, &mut RasterCommands<P>) + 'static,
    {
        self.record_input(
            self.bounds,
            self.clip,
            Recorded { value, prepare, opaque },
            |input, offset, command, _, context| {
                let index = context.commands.len() as usize;
                let value_offset = offset + offset_of!(Recorded<T, F>, value) as u32;
                (input.prepare)(value_offset, command, context.commands);
                if input.opaque.intersection(command.bounds).is_some() {
                    context.commands.metadata[index].coverage = Coverage::Rect;
                    context.commands.entries[index].opaque =
                        Offset::new(offset + offset_of!(Recorded<T, F>, opaque) as u32);
                }
            },
        );
    }

    fn record_input<T: Compare, F>(&mut self, bounds: PhysicalRect, clip: u32, value: T, prepare: F)
    where
        F: Fn(&T, u32, &Command, &DisplayList, &mut DispatchContext<'_, P>) + 'static,
    {
        // safety: this renderer dispatches its list with DispatchContext
        unsafe { self.list.record(bounds, clip, value, prepare) };
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
