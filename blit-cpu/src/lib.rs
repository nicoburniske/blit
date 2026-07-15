mod pixel;
pub mod render;
mod strategy;
mod text;

pub use fontdue::{Font, FontSettings};
pub use pixel::{Pixel, PixelBuffer, PremultipliedRgbaColor, Rgb8Pixel, VecBuffer};
pub use strategy::{Direct, RenderStrategy, Scanline};

use blit::{
    FontId, ImageData, ImageId, LogicalPoint, LogicalRect, LogicalSize, PhysicalRect, TextRequest,
    widgets::{Border, BorderRadius, BoxShadowRequest, ImageRequest, Rectangle},
};
use render::{image, rectangle, shadow};
use strategy::{
    clip::ClipStack,
    command::{CommandList, PreparedText},
};

pub struct RendererConfig {
    pub fonts: Vec<FontFace>,
    pub glyph_cache_capacity: usize,
    pub paragraph_cache_capacity: usize,
    pub shadow_cache_capacity: usize,
}

pub struct FontFace {
    pub id: FontId,
    pub weight: u16,
    pub font: Font,
}

pub struct Renderer<B: PixelBuffer, S: RenderStrategy<B> = Direct> {
    context: RenderContext<B>,
    strategy: S,
}

impl<B: PixelBuffer> Renderer<B, Direct> {
    pub fn new(buffer: B, config: RendererConfig) -> Self {
        Self {
            context: RenderContext {
                buffer,
                scale_factor: 1.0,
                images: SlotMap::with_key(),
                has_dead_images: false,
                shadows: shadow::Cache::new(config.shadow_cache_capacity),
                text: TextRenderer::new(config),
                commands: CommandList::default(),
                clips: ClipStack::default(),
            },
            strategy: Direct,
        }
    }

    pub fn strategy<T: RenderStrategy<B>>(self, strategy: T) -> Renderer<B, T> {
        Renderer {
            context: self.context,
            strategy,
        }
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>> Renderer<B, S> {
    pub fn with_scale_factor(mut self, scale_factor: f32) -> Self {
        assert!(scale_factor.is_finite() && scale_factor > 0.0);
        self.context.scale_factor = scale_factor;
        self
    }

    pub fn screen(&self) -> PhysicalRect {
        PhysicalRect {
            x: 0,
            y: 0,
            width: self.context.buffer.width() as i32,
            height: self.context.buffer.height() as i32,
        }
    }

    pub fn scale_factor(&self) -> f32 {
        self.context.scale_factor
    }

    pub fn buffer(&self) -> &B {
        &self.context.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut B {
        &mut self.context.buffer
    }

    pub fn begin_frame(&mut self) {
        assert!(self.context.commands.is_empty());
    }

    pub fn end_frame(&mut self, damage: &[PhysicalRect]) {
        self.strategy.render(&mut self.context, damage);
        self.context.commands.clear();
        self.context.clips.clear();
        self.context.finish_frame();
    }

    pub fn push_rounded_clip(&mut self, area: LogicalRect, radius: BorderRadius) {
        self.context
            .clips
            .push(area, radius, self.context.scale_factor)
    }

    pub fn pop_rounded_clip(&mut self) {
        self.context.clips.pop()
    }

    pub fn draw_rectangle(&mut self, request: &Rectangle<'_>, clip: PhysicalRect) {
        if let Border::Gradient { width, gradient } = request.border
            && let Some(prepared) =
                rectangle::Gradient::new(request, width, gradient, self.context.scale_factor)
            && let Some(bounds) = prepared.geometry.intersection(clip)
        {
            if self.context.commands.push_gradient_rectangle(
                prepared,
                gradient.stops,
                bounds,
                self.context.clips.current(),
            ) {
                return;
            }
        }
        if let Some(rectangle) = rectangle::Prepared::new(request, self.context.scale_factor)
            && let Some(bounds) = rectangle.geometry.intersection(clip)
        {
            self.context
                .commands
                .push_rectangle(rectangle, bounds, self.context.clips.current());
        }
    }

    pub fn draw_box_shadow(&mut self, request: &BoxShadowRequest, clip: PhysicalRect) {
        let Some(request) = self.context.shadows.prepare(
            &mut self.context.images,
            request,
            self.context.scale_factor,
        ) else {
            return;
        };
        match request {
            shadow::Prepared::Rectangle(rectangle) => self.draw_rectangle(&rectangle, clip),
            shadow::Prepared::Image(request) => {
                let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
                if let Some(texture) = self.context.images.get(image) {
                    image::prepare(&request, &texture.data, clip, 1.0, |image, bounds| {
                        self.context.commands.push_image(
                            image,
                            bounds,
                            self.context.clips.current(),
                            texture.opaque,
                        )
                    });
                }
            }
        }
    }

    pub fn draw_image(&mut self, request: &ImageRequest, clip: PhysicalRect) {
        let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
        if let Some(texture) = self.context.images.get(image) {
            image::prepare(
                request,
                &texture.data,
                clip,
                self.context.scale_factor,
                |image, bounds| {
                    self.context.commands.push_image(
                        image,
                        bounds,
                        self.context.clips.current(),
                        texture.opaque,
                    )
                },
            );
        }
    }

    pub fn draw_text(
        &mut self,
        request: &TextRequest<'_>,
        clip: PhysicalRect,
    ) -> Option<PhysicalRect> {
        let area = request.area.to_physical(self.context.scale_factor);
        let visible_area = area.intersection(clip)?;
        let (paragraph, paragraph_bounds) = self
            .context
            .text
            .prepare(request, self.context.scale_factor);
        let bounds = paragraph_bounds.intersection(visible_area)?;
        self.context.commands.push_text(
            PreparedText {
                paragraph,
                area,
                color: request.color,
            },
            bounds,
            self.context.clips.current(),
        );
        Some(bounds)
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageId {
        data.validate();
        let image = self.context.images.insert(StoredImage::new(data));
        ImageId(image.data().as_ffi())
    }

    pub fn drop_image(&mut self, image: ImageId) {
        let image = RendererImageId::from(KeyData::from_ffi(image.0));
        if let Some(image) = self.context.images.get_mut(image) {
            image.live = false;
            self.context.has_dead_images = true;
        }
    }

    pub fn text_offset_at_position(
        &mut self,
        request: &TextRequest<'_>,
        position: LogicalPoint,
    ) -> usize {
        self.context
            .text
            .offset_at_position(request, position, self.context.scale_factor)
    }

    pub fn measure_text(&mut self, request: &TextRequest<'_>) -> LogicalSize {
        self.context
            .text
            .measure(request, self.context.scale_factor)
    }

    pub fn measure_text_height(&mut self, request: &TextRequest<'_>) -> f32 {
        self.context
            .text
            .measure_height(request, self.context.scale_factor)
    }

    pub fn text_cursor_rect(
        &mut self,
        request: &TextRequest<'_>,
        byte_offset: usize,
    ) -> LogicalRect {
        self.context
            .text
            .cursor_rect(request, byte_offset, self.context.scale_factor)
    }
}

use pixel::PixelSpan;
use slotmap::{Key, KeyData, SlotMap, new_key_type};
use text::TextRenderer;

new_key_type! {
    pub struct RendererImageId;
}

#[doc(hidden)]
pub struct RenderContext<B: PixelBuffer> {
    buffer: B,
    scale_factor: f32,
    images: SlotMap<RendererImageId, StoredImage>,
    has_dead_images: bool,
    shadows: shadow::Cache,
    text: TextRenderer,
    commands: CommandList,
    clips: ClipStack,
}

pub struct StoredImage {
    data: ImageData,
    opaque: bool,
    live: bool,
}

impl StoredImage {
    fn new(data: ImageData) -> Self {
        let width = data.texture_rect.width as usize;
        let height = data.texture_rect.height as usize;
        let bytes = data.pixels.bytes();
        let opaque = match data.format {
            blit::ImageFormat::Rgb8 => true,
            blit::ImageFormat::Rgba8 | blit::ImageFormat::Rgba8Premultiplied => {
                (0..height).all(|line| {
                    bytes[line * data.stride_bytes..][..width * 4]
                        .chunks_exact(4)
                        .all(|pixel| pixel[3] == 255)
                })
            }
            blit::ImageFormat::Alpha8(_) => (0..height).all(|line| {
                bytes[line * data.stride_bytes..][..width]
                    .iter()
                    .all(|alpha| *alpha == 255)
            }),
        };
        Self {
            data,
            opaque,
            live: true,
        }
    }
}

impl<B: PixelBuffer> RenderContext<B> {
    fn finish_frame(&mut self) {
        self.shadows.finish_frame(&mut self.images);
        self.text.finish_frame();
        if self.has_dead_images {
            self.images.retain(|_, image| image.live);
            self.has_dead_images = false;
        }
    }
}

#[cfg(test)]
mod test;
