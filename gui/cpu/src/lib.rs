#![feature(portable_simd)]

mod damage;
mod glyph;
mod pixel;
mod render;
mod strategy;
mod text;

use blit::{PhysicalRect, Scale2};
use blit_arena::Arena;
use blit_arrayvec::ArrayVec;
use blit_gui::{
    RenderInput, TextSystem,
    color::Color,
    display_list::{BoxShadow, Command, DisplayList, Rectangle},
    image::{ImageData, ImageFormat, ImageHandle, ImageRequest},
    style::Border,
    text::{TextPhases, TextRequest},
};
use self::{
    damage::MAX_DAMAGE,
    pixel::PixelSpan,
    render::{image as render_image, image_patch::AlphaRows, rectangle, shadow, triangle},
    strategy::{
        clip::ClipStack,
        command::{CommandList, PreparedText},
    },
    text::TextRenderer,
};

pub use self::{
    pixel::{Argb8888, Pixel, PixelBuffer, PremultipliedRgbaColor, Rgb8Pixel, Rgba8888, VecBuffer, Xrgb8888},
    strategy::{Direct, RenderStrategy, Scanline},
};

#[derive(Clone, Copy, Debug)]
pub struct RendererConfig {
    /// maximum retained text paint data in bytes
    pub paint_cache_capacity: usize,
    /// maximum retained rasterized glyph data in bytes
    pub glyph_cache_capacity: usize,
    /// maximum retained shadow masks in bytes
    pub shadow_cache_capacity: usize,
    /// cached horizontal positions for grayscale glyph rasterization
    pub text_phases: TextPhases,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            paint_cache_capacity: 2 * 1024 * 1024,
            glyph_cache_capacity: 1024 * 1024,
            shadow_cache_capacity: 512 * 1024,
            text_phases: TextPhases::default(),
        }
    }
}

pub struct Renderer<B: PixelBuffer, S: RenderStrategy<B> = Direct> {
    context: RenderContext<B>,
    strategy: S,
    previous: DisplayList,
    damage: ArrayVec<PhysicalRect, { MAX_DAMAGE * 2 }>,
    previous_damage: ArrayVec<PhysicalRect, MAX_DAMAGE>,
    invalidated: bool,
    scale: Scale2,
}

impl<B: PixelBuffer> Renderer<B, Direct> {
    pub fn new(buffer: B, config: RendererConfig) -> Self {
        Self {
            context: RenderContext {
                buffer,
                scale_factor: 1.0,
                images: Arena::new(),
                shadows: shadow::Cache::new(config.shadow_cache_capacity),
                text: TextRenderer::new(&config),
                commands: CommandList::default(),
                clips: ClipStack::default(),
            },
            strategy: Direct::default(),
            previous: DisplayList::default(),
            damage: ArrayVec::new(),
            previous_damage: ArrayVec::new(),
            invalidated: true,
            scale: Scale2::IDENTITY,
        }
    }

    pub fn strategy<T: RenderStrategy<B>>(self, strategy: T) -> Renderer<B, T> {
        let Self {
            context,
            previous,
            damage,
            previous_damage,
            invalidated,
            scale,
            ..
        } = self;
        Renderer {
            context,
            strategy,
            previous,
            damage,
            previous_damage,
            invalidated,
            scale,
        }
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>> Renderer<B, S> {
    /// creates an image for use with this renderer
    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        data.validate();
        let id = self
            .context
            .images
            .insert_with(|id| StoredImage::new(ImageHandle::new(id.into(), data.size), data));
        self.context.images.get(id).unwrap().handle.clone()
    }

    pub fn render(&mut self, input: RenderInput<'_>) {
        let RenderInput {
            display_list,
            text,
            scale,
            ..
        } = input;
        if self.scale != scale {
            self.scale = scale;
            self.set_scale(scale);
            self.invalidate_all();
        }

        let mut damage = std::mem::take(&mut self.damage);
        damage.clear();
        if std::mem::take(&mut self.invalidated) {
            damage.push(self.screen());
        } else {
            damage::diff(&self.previous, display_list, &mut damage);
        }
        let current_damage = damage.len();
        damage.extend_from_slice(&self.previous_damage);
        self.render_damage(text, display_list, &damage);
        self.previous_damage.clear();
        self.previous_damage.extend_from_slice(&damage[..current_damage]);
        std::mem::swap(&mut self.previous, display_list);
        self.damage = damage;
    }

    pub fn screen(&self) -> PhysicalRect {
        PhysicalRect {
            x: 0,
            y: 0,
            width: self.context.buffer.width() as i32,
            height: self.context.buffer.height() as i32,
        }
    }

    pub fn buffer(&self) -> &B {
        &self.context.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut B {
        &mut self.context.buffer
    }

    pub fn invalidate_all(&mut self) {
        self.invalidated = true;
        self.previous_damage.clear();
    }

    #[doc(hidden)]
    pub fn render_damage(&mut self, text: &mut TextSystem, display_list: &DisplayList, damage: &[PhysicalRect]) {
        assert!(self.context.commands.is_empty());
        if !damage.is_empty() {
            for clip in display_list.clips() {
                self.context
                    .clips
                    .push_node(clip.parent.0, clip.area, clip.radius, self.context.scale_factor);
            }
            for record in display_list.iter() {
                if !damage
                    .iter()
                    .any(|damage| record.bounds.intersection(*damage).is_some())
                {
                    continue;
                }
                match record.command {
                    Command::Clear => self.context.commands.push_clear(record.bounds),
                    Command::Rectangle(rectangle) => self.prepare_rectangle(&rectangle, record.bounds, record.clip.0),
                    Command::Image(image) => self.prepare_image(&image, record.bounds, record.clip.0),
                    Command::Text(request, colors) => {
                        self.prepare_text(text, &request, colors, record.bounds, record.clip.0);
                    }
                    Command::BoxShadow(shadow) => self.prepare_box_shadow(&shadow, record.bounds, record.clip.0),
                    Command::Mesh(mesh) => {
                        for indices in mesh.indices.as_chunks::<3>().0 {
                            let vertices = indices.map(|index| mesh.vertices[index as usize]);
                            if let Some((triangle, bounds)) =
                                triangle::Prepared::new(vertices, self.scale, record.bounds, damage)
                            {
                                self.context.commands.push_triangle(triangle, bounds, record.clip.0);
                            }
                        }
                    }
                    _ => {}
                }
            }
            self.strategy.render(&mut self.context, damage);
        }
        self.context.commands.clear();
        self.context.clips.clear();
        self.context.finish_frame();
    }
}

#[doc(hidden)]
pub struct RenderContext<B: PixelBuffer> {
    buffer: B,
    scale_factor: f32,
    images: Arena<StoredImage>,
    shadows: shadow::Cache,
    text: TextRenderer,
    commands: CommandList,
    clips: ClipStack,
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
    fn finish_frame(&mut self) {
        self.shadows.finish_frame();
        self.text.finish_frame();
        self.images.retain(|image| !image.handle.is_uniquely_owned());
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>> Renderer<B, S> {
    fn set_scale(&mut self, scale: Scale2) {
        assert_eq!(scale.x, scale.y, "CPU rendering requires uniform scale");
        assert!(scale.x.is_finite() && scale.x > 0.0);
        self.context.scale_factor = scale.x;
    }

    fn prepare_rectangle(&mut self, request: &Rectangle<'_>, bounds: PhysicalRect, clip: u32) {
        if let Border::Gradient { width, gradient } = request.border
            && let Some(prepared) = rectangle::Gradient::new(request, width, gradient, self.context.scale_factor)
            && let Some(bounds) = prepared.geometry.intersection(bounds)
        {
            if self
                .context
                .commands
                .push_gradient_rectangle(prepared, gradient.stops, bounds, clip)
            {
                return;
            }
        }
        if let Some(rectangle) = rectangle::Prepared::new(request, self.context.scale_factor)
            && let Some(bounds) = rectangle.geometry.intersection(bounds)
        {
            self.context.commands.push_rectangle(rectangle, bounds, clip);
        }
    }

    fn prepare_box_shadow(&mut self, shadow: &BoxShadow, bounds: PhysicalRect, clip: u32) {
        let Some(request) = self
            .context
            .shadows
            .prepare(&mut self.context.images, shadow, self.context.scale_factor)
        else {
            return;
        };
        match request {
            shadow::Prepared::Rectangle(rectangle) => self.prepare_rectangle(&rectangle, bounds, clip),
            shadow::Prepared::Image(request) => {
                if let Some(texture) = self.context.images.get(request.image.into()) {
                    render_image::prepare(&request, &texture.data, bounds, 1.0, |image, bounds| {
                        self.context.commands.push_image(
                            image,
                            bounds,
                            clip,
                            image.is_opaque(&texture.data, texture.opaque),
                            image.has_opaque_spans(&texture.data, texture.has_opaque_spans),
                        )
                    });
                }
            }
        }
    }

    fn prepare_image(&mut self, request: &ImageRequest, bounds: PhysicalRect, clip: u32) {
        if let Some(texture) = self.context.images.get(request.image.into()) {
            render_image::prepare(
                request,
                &texture.data,
                bounds,
                self.context.scale_factor,
                |image, bounds| {
                    self.context.commands.push_image(
                        image,
                        bounds,
                        clip,
                        image.is_opaque(&texture.data, texture.opaque),
                        image.has_opaque_spans(&texture.data, texture.has_opaque_spans),
                    )
                },
            );
        }
    }

    fn prepare_text(
        &mut self,
        text: &mut TextSystem,
        request: &TextRequest,
        colors: &[Option<Color>],
        bounds: PhysicalRect,
        clip: u32,
    ) -> Option<PhysicalRect> {
        let area = request.area.to_physical(Scale2::uniform(self.context.scale_factor));
        let visible_area = area.intersection(bounds)?;
        let (glyph_start, glyph_end, runs, paragraph_bounds) =
            self.context
                .text
                .prepare(text, request, colors, self.context.scale_factor);
        let bounds = paragraph_bounds.intersection(visible_area)?;
        self.context.commands.push_text(
            PreparedText {
                glyph_start,
                glyph_end,
                runs,
                area,
                color: request.color,
            },
            bounds,
            clip,
        );
        Some(bounds)
    }
}

#[cfg(test)]
mod test;
