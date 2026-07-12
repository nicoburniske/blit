mod image;
mod pixel;
mod rectangle;
mod strategy;
mod text;

pub use fontdue::{Font, FontSettings};
pub use pixel::{Pixel, PixelBuffer, PremultipliedRgbaColor, Rgb8Pixel, VecBuffer};
pub use strategy::{Direct, RenderStrategy, Scanline};

use blit::{
    FontId, ImageData, ImageId, LogicalPoint, LogicalRect, PhysicalRect, TextRequest,
    widgets::{ImageRequest, Rectangle},
};

pub struct RendererConfig {
    pub fonts: Vec<FontFace>,
    pub glyph_cache_capacity: usize,
    pub paragraph_cache_capacity: usize,
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
                text: TextRenderer::new(config),
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
        self.strategy.begin_frame(&mut self.context)
    }

    pub fn end_frame(&mut self) {
        self.strategy.end_frame(&mut self.context);
        self.context.finish_frame();
    }

    pub fn draw_rectangle(&mut self, request: &Rectangle, clips: &[PhysicalRect]) {
        self.strategy
            .draw_rectangle(&mut self.context, request, clips)
    }

    pub fn draw_image(&mut self, request: &ImageRequest, clips: &[PhysicalRect]) {
        self.strategy.draw_image(&mut self.context, request, clips)
    }

    pub fn draw_text(&mut self, request: &TextRequest<'_>, clips: &[PhysicalRect]) {
        self.strategy.draw_text(&mut self.context, request, clips)
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageId {
        data.validate();
        let image = self.context.images.insert(StoredImage { data, live: true });
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
    struct RendererImageId;
}

#[doc(hidden)]
pub struct RenderContext<B: PixelBuffer> {
    buffer: B,
    scale_factor: f32,
    images: SlotMap<RendererImageId, StoredImage>,
    has_dead_images: bool,
    text: TextRenderer,
}

struct StoredImage {
    data: ImageData,
    live: bool,
}

impl<B: PixelBuffer> RenderContext<B> {
    fn finish_frame(&mut self) {
        self.text.finish_frame();
        if self.has_dead_images {
            self.images.retain(|_, image| image.live);
            self.has_dead_images = false;
        }
    }
}

#[cfg(test)]
mod test;
