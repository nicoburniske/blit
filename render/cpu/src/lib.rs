#![feature(portable_simd)]

pub mod color;
pub mod command_list;
mod glyph;
pub mod image;
mod pixel;
mod raster;
pub mod render;
mod strategy;
pub mod style;
mod text;
pub mod text_types;

use crate::{
    command_list::{BoxShadow, Command, CommandList as ResolvedCommandList, Rectangle},
    image::{ImageData, ImageHandle, ImageId, ImageRequest},
    style::Border,
    text_types::{FontId, TextLayoutRequest, TextRequest, TextRunId, TextStyle},
};
use blit::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect, Scale2};
pub use blit_text::{FontData, FontError, FontId as BackendFontId, TextSystem};
pub use pixel::{
    Argb8888, Pixel, PixelBuffer, PremultipliedRgbaColor, Rgb8Pixel, Rgba8888, VecBuffer, Xrgb8888,
};
use render::{image as render_image, image_patch::AlphaRows, rectangle, shadow};
pub use strategy::{Direct, RenderStrategy, Scanline};
use strategy::{
    clip::ClipStack,
    command::{CommandList, PreparedText},
};

pub struct RendererConfig {
    pub fonts: Vec<FontFace>,
    pub text_cache_capacity: usize,
    pub layout_cache_capacity: usize,
    pub glyph_cache_capacity: usize,
    pub shadow_cache_capacity: usize,
}

pub struct FontFace {
    pub id: FontId,
    pub weight: u16,
    pub font: BackendFontId,
}

pub struct Renderer<B: PixelBuffer, S: RenderStrategy<B> = Direct> {
    context: RenderContext<B>,
    strategy: S,
}

impl<B: PixelBuffer> Renderer<B, Direct> {
    pub fn new(buffer: B, config: RendererConfig, text: TextSystem) -> Self {
        let shadow_cache_capacity = config.shadow_cache_capacity;
        Self {
            context: RenderContext {
                buffer,
                scale_factor: 1.0,
                images: SlotMap::with_key(),
                shadows: shadow::Cache::new(shadow_cache_capacity),
                text: TextRenderer::new(config, text),
                commands: CommandList::default(),
                clips: ClipStack::default(),
            },
            strategy: Direct::default(),
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

    fn prepare_rectangle(&mut self, request: &Rectangle<'_>, bounds: PhysicalRect, clip: u32) {
        if let Border::Gradient { width, gradient } = request.border
            && let Some(prepared) =
                rectangle::Gradient::new(request, width, gradient, self.context.scale_factor)
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
            self.context
                .commands
                .push_rectangle(rectangle, bounds, clip);
        }
    }

    fn prepare_box_shadow(&mut self, shadow: &BoxShadow, bounds: PhysicalRect, clip: u32) {
        let Some(request) = self.context.shadows.prepare(
            &mut self.context.images,
            shadow,
            self.context.scale_factor,
        ) else {
            return;
        };
        match request {
            shadow::Prepared::Rectangle(rectangle) => {
                self.prepare_rectangle(&rectangle, bounds, clip)
            }
            shadow::Prepared::Image(request) => {
                let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
                if let Some(texture) = self.context.images.get(image) {
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
        let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
        if let Some(texture) = self.context.images.get(image) {
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
        request: &TextRequest,
        bounds: PhysicalRect,
        clip: u32,
    ) -> Option<PhysicalRect> {
        let area = request
            .area
            .to_physical(Scale2::uniform(self.context.scale_factor));
        let visible_area = area.intersection(bounds)?;
        let (glyph_start, glyph_end, lines, paragraph_bounds) = self
            .context
            .text
            .prepare(request, self.context.scale_factor);
        let bounds = paragraph_bounds.intersection(visible_area)?;
        self.context.commands.push_text(
            PreparedText {
                glyph_start,
                glyph_end,
                lines,
                area,
                color: request.color,
            },
            bounds,
            clip,
        );
        Some(bounds)
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
    shadows: shadow::Cache,
    text: TextRenderer,
    commands: CommandList,
    clips: ClipStack,
}

pub struct StoredImage {
    handle: ImageHandle,
    data: ImageData,
    alpha_rows: AlphaRows,
    has_opaque_spans: bool,
    opaque: bool,
}

impl StoredImage {
    fn insert(images: &mut SlotMap<RendererImageId, StoredImage>, data: ImageData) -> ImageHandle {
        data.validate();
        let size = data.size;
        let image = images.insert_with_key(|id| {
            let handle = ImageHandle::new(ImageId(id.data().as_ffi()), size);
            Self::new(handle, data)
        });
        images[image].handle.clone()
    }

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
            crate::image::ImageFormat::Rgb8 | crate::image::ImageFormat::Luma8 => {
                (AlphaRows::default(), true)
            }
            crate::image::ImageFormat::Rgba8 => (AlphaRows::default(), rgba_opaque()),
            crate::image::ImageFormat::Rgba8Premultiplied if rgba_opaque() => {
                (AlphaRows::default(), true)
            }
            crate::image::ImageFormat::Rgba8Premultiplied if width > u16::MAX as usize => {
                (AlphaRows::default(), false)
            }
            crate::image::ImageFormat::Rgba8Premultiplied => {
                let mut rows = Vec::with_capacity(height * 4);
                for y in 0..height {
                    let row = &bytes[y * data.stride_bytes..][..width * 4];
                    let mut visible_start = width;
                    let mut visible_end = 0;
                    let mut run_start = 0;
                    let mut opaque_start = 0;
                    let mut opaque_end = 0;
                    for (x, alpha) in row
                        .chunks_exact(4)
                        .map(|pixel| pixel[3])
                        .chain([0])
                        .enumerate()
                    {
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
            crate::image::ImageFormat::Alpha8(_)
                if (0..height).all(|line| {
                    bytes[line * data.stride_bytes..][..width]
                        .iter()
                        .all(|alpha| *alpha == 255)
                }) =>
            {
                (AlphaRows::default(), true)
            }
            crate::image::ImageFormat::Alpha8(_) if width > u16::MAX as usize => {
                (AlphaRows::default(), false)
            }
            crate::image::ImageFormat::Alpha8(_) => {
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
        self.images
            .retain(|_, image| !image.handle.is_uniquely_owned());
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>> Renderer<B, S> {
    pub fn set_scale(&mut self, scale: Scale2) {
        assert_eq!(scale.x, scale.y, "CPU rendering requires uniform scale");
        assert!(scale.x.is_finite() && scale.x > 0.0);
        self.context.scale_factor = scale.x;
    }

    pub fn render(&mut self, commands: &ResolvedCommandList, damage: &[PhysicalRect]) {
        assert!(self.context.commands.is_empty());
        if !damage.is_empty() {
            for clip in commands.clips() {
                self.context.clips.push_node(
                    clip.parent.0,
                    clip.area,
                    clip.radius,
                    self.context.scale_factor,
                );
            }
            for record in commands.iter() {
                if !damage
                    .iter()
                    .any(|damage| record.bounds.intersection(*damage).is_some())
                {
                    continue;
                }
                match record.command {
                    Command::Clear => self.context.commands.push_clear(record.bounds),
                    Command::Rectangle(rectangle) => {
                        self.prepare_rectangle(&rectangle, record.bounds, record.clip.0)
                    }
                    Command::Image(image) => {
                        self.prepare_image(&image, record.bounds, record.clip.0)
                    }
                    Command::Text(text) => {
                        self.prepare_text(&text, record.bounds, record.clip.0);
                    }
                    Command::BoxShadow(shadow) => {
                        self.prepare_box_shadow(&shadow, record.bounds, record.clip.0)
                    }
                }
            }
            self.strategy.render(&mut self.context, damage);
        }
        self.context.commands.clear();
        self.context.clips.clear();
        self.context.finish_frame();
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        StoredImage::insert(&mut self.context.images, data)
    }

    pub fn text_run(&mut self, text: &str, style: TextStyle) -> TextRunId {
        self.context
            .text
            .text_run(text, style, self.context.scale_factor)
    }

    pub fn text_offset_at_position(
        &mut self,
        request: &TextRequest,
        position: LogicalPoint,
    ) -> usize {
        self.context
            .text
            .offset_at_position(request, position, self.context.scale_factor)
    }

    pub fn measure_text(&mut self, request: &TextLayoutRequest) -> LogicalSize {
        self.context
            .text
            .measure(request, self.context.scale_factor)
    }

    pub fn text_cursor_rect(&mut self, request: &TextRequest, byte_offset: usize) -> LogicalRect {
        self.context
            .text
            .cursor_rect(request, byte_offset, self.context.scale_factor)
    }
}

#[cfg(test)]
mod test;
