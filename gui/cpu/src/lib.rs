pub mod damage;
pub mod extension;
mod glyph;
mod pixel;
pub mod raster;
mod render;
pub mod shadow;
mod strategy;
mod text;

use std::any::TypeId;

use blit::{LogicalRect, Paint, PhysicalRect, Scale2};
use blit_arena::Arena;
use blit_gui::{
    ClipContext, PaintCustom, TextContext, TextSystem,
    atom::Clear,
    display_list::{ClipId, Command, DisplayList, Mesh, Rectangle, arena::Writer},
    image::{ImageData, ImageFormat, ImageHandle, ImageRequest},
    style::BorderRadius,
    text::TextRequest,
};

pub use self::{
    damage::{DamageTracker, tile::TileDamage},
    pixel::{Argb8888, Pixel, PixelBuffer, PremultipliedRgbaColor, Rgb8Pixel, Rgba8888, VecBuffer, Xrgb8888},
    strategy::{Direct, RenderStrategy, Scanline},
};
use self::{
    damage::{frame::FrameDamage, passes::PassDamage},
    extension::registry::{ReadRequest, Registry},
    pixel::PixelSpan,
    render::{image as render_image, image_patch::AlphaRows, rectangle, triangle},
    strategy::{clip::ClipStack, command::CommandList},
    text::TextRenderer,
};

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

pub struct Renderer<B: PixelBuffer, S: RenderStrategy<B> = Direct, D: DamageTracker = TileDamage> {
    context: RenderContext<B>,
    display_list: DisplayList,
    text_system: TextSystem,
    paint_clip: ClipId,
    paint_clips: Vec<ClipId>,
    strategy: S,
    damage: D,
    pass_damage: PassDamage,
    pass_shift: u32,
}

impl<B: PixelBuffer> Renderer<B, Direct> {
    pub fn new(buffer: B, text: TextSystem, config: RendererConfig) -> Self {
        Self {
            context: RenderContext {
                buffer,
                scale_factor: 1.0,
                images: Arena::new(),
                text: TextRenderer::new(&config),
                commands: CommandList::default(),
                clips: ClipStack::default(),
                extensions: Registry::new(),
            },
            display_list: DisplayList::default(),
            text_system: text,
            paint_clip: ClipId::default(),
            paint_clips: Vec::new(),
            strategy: Direct::default(),
            damage: TileDamage::new(config.damage_tile_size),
            pass_damage: PassDamage::default(),
            pass_shift: config.damage_tile_size.trailing_zeros(),
        }
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Renderer<B, S, D> {
    pub fn register<E: extension::Extension<B::Pixel>>(&mut self, extension: E) {
        self.context.extensions.register(extension);
        self.damage.invalidate();
    }

    pub fn strategy<T: RenderStrategy<B>>(self, strategy: T) -> Renderer<B, T, D> {
        let Self {
            context,
            display_list,
            text_system,
            paint_clip,
            paint_clips,
            damage,
            pass_damage,
            pass_shift,
            ..
        } = self;
        Renderer {
            context,
            display_list,
            text_system,
            paint_clip,
            paint_clips,
            strategy,
            damage,
            pass_damage,
            pass_shift,
        }
    }

    pub fn damage_tracker<T: DamageTracker>(self, mut damage: T) -> Renderer<B, S, T> {
        damage.invalidate();
        Renderer {
            context: self.context,
            display_list: self.display_list,
            text_system: self.text_system,
            paint_clip: self.paint_clip,
            paint_clips: self.paint_clips,
            strategy: self.strategy,
            damage,
            pass_damage: self.pass_damage,
            pass_shift: self.pass_shift,
        }
    }

    /// creates an image for use with this renderer
    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        data.validate();
        let id = self
            .context
            .images
            .insert_with(|id| StoredImage::new(ImageHandle::new(id.into(), data.size), data));
        self.context.images.get(id).unwrap().handle.clone()
    }

    /// renders into a reused presentation buffer
    ///
    /// buffer age is the number of presentations since this buffer was last used
    /// zero means its contents are unknown and forces a full repaint
    pub fn render(&mut self, buffer_age: u8) {
        assert!(self.paint_clips.is_empty(), "unbalanced paint clips");
        self.draw(buffer_age);
        self.display_list.clear();
        self.paint_clip = ClipId::default();
        self.text_system.finish_frame();
    }

    fn draw(&mut self, buffer_age: u8) {
        let scale = self.context.scale_factor;
        self.damage.update(&self.display_list, self.screen(), scale, buffer_age);
        let screen = self.screen();
        self.pass_damage.update(
            &self.display_list,
            &self.context.extensions,
            &self.damage,
            screen,
            scale,
            self.pass_shift,
        );
        let plan = &self.pass_damage;
        let mut damage = FrameDamage {
            tracker: &mut self.damage,
            region: plan.stages.first(),
        };
        let context = &mut self.context;
        assert!(context.commands.is_empty());
        if !damage.is_empty() {
            if let Some(region) = plan.stages.first() {
                context
                    .extensions
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
            for clip in self.display_list.clips() {
                context
                    .clips
                    .push_node(clip.parent.0, clip.area, clip.radius, context.scale_factor);
            }
            let mut stage = 0;
            for (index, record) in self.display_list.iter().enumerate() {
                if let Some(node) = plan.passes.get(stage).filter(|node| node.index == index) {
                    self.strategy.render(context, &damage);
                    context.commands.clear();
                    stage += 1;
                    damage.region = Some(&plan.stages[stage]);
                    if damage.intersects(node.bounds, record.clip.0) {
                        let Command::Custom(command) = record.command else {
                            unreachable!()
                        };
                        let id = context.extensions.prepare(
                            node.handler,
                            command,
                            context.scale_factor,
                            record.bounds,
                            &mut context.images,
                        );
                        context
                            .extensions
                            .read(&mut context.buffer, ReadRequest::Snapshot(&node.reads));
                        context.extensions.pass(
                            id,
                            &mut context.buffer,
                            &context.clips,
                            node.bounds,
                            record.clip.0,
                            &context.images,
                            &plan.stages[stage],
                        );
                    }
                    continue;
                }
                if !damage.intersects(record.bounds, record.clip.0) {
                    continue;
                }
                match record.command {
                    Command::Clear => context.commands.push_clear(record.bounds),
                    Command::Rectangle(rectangle) => {
                        context.prepare_rectangle(&rectangle, record.bounds, record.clip.0)
                    }
                    Command::Image(image) => context.prepare_image(&image, record.bounds, record.clip.0),
                    Command::Text(request) => {
                        context.prepare_text(&mut self.text_system, &request, record.bounds, record.clip.0);
                    }
                    Command::Mesh(mesh) => {
                        for indices in mesh.indices.as_chunks::<3>().0 {
                            let vertices = indices.map(|index| mesh.vertices[index as usize]);
                            if let Some((triangle, bounds)) = triangle::Prepared::new(
                                vertices,
                                Scale2::uniform(context.scale_factor),
                                record.bounds,
                                record.clip.0,
                                &damage,
                            ) {
                                context.commands.push_triangle(triangle, bounds, record.clip.0);
                            }
                        }
                    }
                    Command::Custom(command) => {
                        let handler = context.extensions.index(command);
                        if context.extensions.entries[handler]
                            .1
                            .read_bounds(command, record.bounds, context.scale_factor)
                            .is_some()
                        {
                            continue;
                        }
                        let id = context.extensions.prepare(
                            handler,
                            command,
                            context.scale_factor,
                            record.bounds,
                            &mut context.images,
                        );
                        context.commands.push_custom(id, record.bounds, record.clip.0);
                    }
                    _ => {}
                }
            }
            self.strategy.render(context, &damage);
            if !plan.passes.is_empty() {
                context.extensions.restore(&mut context.buffer, &plan.preserve);
            }
        }
        context.commands.clear();
        context.clips.clear();
        context.extensions.finish_frame();
        context.text.finish_frame(&mut self.text_system);
        context.images.retain(|image| !image.handle.is_uniquely_owned());
        self.damage.finish_frame(&mut self.display_list);
    }

    pub fn screen(&self) -> PhysicalRect {
        PhysicalRect {
            x: 0,
            y: 0,
            width: i32::try_from(self.context.buffer.width()).expect("buffer width exceeds physical coordinates"),
            height: i32::try_from(self.context.buffer.height()).expect("buffer height exceeds physical coordinates"),
        }
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
        if self.context.scale_factor != scale {
            self.context.scale_factor = scale;
            self.invalidate_all();
        }
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> blit::Context for Renderer<B, S, D> {
    type Scalar = f32;
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<Rectangle<'_>> for Renderer<B, S, D> {
    fn paint(&mut self, bounds: LogicalRect, command: Rectangle<'_>) {
        self.display_list.push_rectangle(
            command,
            bounds.to_physical(Scale2::uniform(self.context.scale_factor)),
            self.paint_clip,
        );
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<ImageRequest> for Renderer<B, S, D> {
    fn paint(&mut self, bounds: LogicalRect, command: ImageRequest) {
        self.display_list.push_image(
            command,
            bounds.to_physical(Scale2::uniform(self.context.scale_factor)),
            self.paint_clip,
        );
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<TextRequest<'_>> for Renderer<B, S, D> {
    fn paint(&mut self, bounds: LogicalRect, command: TextRequest<'_>) {
        self.display_list.push_text(
            command,
            bounds.to_physical(Scale2::uniform(self.context.scale_factor)),
            self.paint_clip,
        );
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<Mesh<'_>> for Renderer<B, S, D> {
    fn paint(&mut self, _: LogicalRect, command: Mesh<'_>) {
        self.display_list
            .push_mesh(command, Scale2::uniform(self.context.scale_factor), self.paint_clip);
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<Clear> for Renderer<B, S, D> {
    fn paint(&mut self, bounds: LogicalRect, _: Clear) {
        self.display_list
            .push_clear(bounds.to_physical(Scale2::uniform(self.context.scale_factor)));
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker, C: extension::Command> Paint<C> for Renderer<B, S, D>
where
    C::Renderer: extension::Extension<B::Pixel, Command = C>,
{
    fn paint(&mut self, bounds: LogicalRect, command: C) {
        self.paint_with(bounds, |_| command);
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker, C: extension::Command> PaintCustom<C> for Renderer<B, S, D>
where
    C::Renderer: extension::Extension<B::Pixel, Command = C>,
{
    fn paint_with(&mut self, bounds: LogicalRect, build: impl FnOnce(&mut Writer<'_>) -> C) {
        if !self
            .context
            .extensions
            .entries
            .iter()
            .any(|(kind, _)| *kind == TypeId::of::<C>())
        {
            self.register(C::Renderer::default());
        }
        self.display_list.push_custom(
            bounds.to_physical(Scale2::uniform(self.context.scale_factor)),
            self.paint_clip,
            build,
        );
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> TextContext for Renderer<B, S, D> {
    fn text(&mut self) -> &mut TextSystem {
        &mut self.text_system
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> ClipContext for Renderer<B, S, D> {
    fn push_clip(&mut self, area: LogicalRect, radius: BorderRadius) {
        self.paint_clips.push(self.paint_clip);
        self.paint_clip = self.display_list.push_clip(self.paint_clip, area, radius);
    }

    fn pop_clip(&mut self) {
        self.paint_clip = self.paint_clips.pop().expect("no paint clip to pop");
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
    extensions: Registry<B::Pixel>,
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

impl<B: PixelBuffer> RenderContext<B> {
    fn prepare_rectangle(&mut self, request: &Rectangle<'_>, bounds: PhysicalRect, clip: u32) {
        if let Some(rectangle) = rectangle::Prepared::new(request, self.scale_factor, &mut self.commands.gradients)
            && let Some(bounds) = rectangle.geometry.intersection(bounds)
        {
            self.commands.push_rectangle(rectangle, bounds, clip);
        }
    }

    fn prepare_image(&mut self, request: &ImageRequest, bounds: PhysicalRect, clip: u32) {
        if let Some(texture) = self.images.get(request.image.into()) {
            render_image::prepare(request, &texture.data, bounds, self.scale_factor, |image, bounds| {
                self.commands.push_image(
                    image,
                    bounds,
                    clip,
                    image.is_opaque(&texture.data, texture.opaque),
                    image.has_opaque_spans(&texture.data, texture.has_opaque_spans),
                )
            });
        }
    }

    fn prepare_text(
        &mut self,
        text: &mut TextSystem,
        request: &TextRequest<'_>,
        bounds: PhysicalRect,
        clip: u32,
    ) -> Option<PhysicalRect> {
        let area = request.area.to_physical(Scale2::uniform(self.scale_factor));
        let visible_area = area.intersection(bounds)?;
        let (prepared, paragraph_bounds) = self.text.prepare(text, request, self.scale_factor);
        let bounds = paragraph_bounds.intersection(visible_area)?;
        self.commands.push_text(prepared, bounds, clip);
        Some(bounds)
    }
}

#[cfg(test)]
mod test;
