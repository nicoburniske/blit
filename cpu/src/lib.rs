mod cache;
mod pixel;
pub mod render;
mod strategy;
mod text;

use blit::{
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    paint::{Border, BoxShadow, FontId, ImageRequest, Rectangle, TextRequest},
    paint_list::{Command, PaintList},
    resource::{ImageData, ImageId, StringData, StringId},
};
pub use blit_font::Font;
pub use pixel::{Pixel, PixelBuffer, PremultipliedRgbaColor, Rgb8Pixel, VecBuffer};
use render::{image, image_patch::AlphaRow, rectangle, shadow};
use strategy::{
    clip::ClipStack,
    command::{CommandList, PreparedText},
};
pub use strategy::{Direct, RenderStrategy, Scanline};

pub struct RendererConfig {
    pub fonts: Vec<FontFace>,
    pub font_metric_cache_capacity: usize,
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
                strings: SlotMap::with_key(),
                has_dead_strings: false,
                shadows: shadow::Cache::new(config.shadow_cache_capacity),
                text: TextRenderer::new(config),
                commands: CommandList::default(),
                clips: ClipStack::default(),
            },
            strategy: Direct::default(),
        }
    }

    pub fn strategy<T: RenderStrategy<B>>(self, strategy: T) -> Renderer<B, T> {
        Renderer { context: self.context, strategy }
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

    pub fn scale_factor(&self) -> f32 { self.context.scale_factor }

    pub fn buffer(&self) -> &B { &self.context.buffer }

    pub fn buffer_mut(&mut self) -> &mut B { &mut self.context.buffer }

    pub fn render(&mut self, paint: &PaintList, damage: &[PhysicalRect]) {
        assert!(self.context.commands.is_empty());
        if !damage.is_empty() {
            for clip in paint.clips() {
                self.context.clips.push_node(
                    clip.parent.0,
                    clip.area,
                    clip.radius,
                    self.context.scale_factor,
                );
            }
            for record in paint.iter() {
                if !damage.iter().any(|damage| record.bounds.intersection(*damage).is_some()) {
                    continue;
                }
                match record.command {
                    Command::Rectangle(rectangle) => {
                        self.prepare_rectangle(&rectangle, record.bounds, record.clip.0)
                    }
                    Command::Image(image) => self.prepare_image(&image, record.bounds, record.clip.0),
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

    fn prepare_rectangle(&mut self, request: &Rectangle<'_>, bounds: PhysicalRect, clip: u16) {
        if let Border::Gradient { width, gradient } = request.border
            && let Some(prepared) =
                rectangle::Gradient::new(request, width, gradient, self.context.scale_factor)
            && let Some(bounds) = prepared.geometry.intersection(bounds)
        {
            if self.context.commands.push_gradient_rectangle(prepared, gradient.stops, bounds, clip) {
                return;
            }
        }
        if let Some(rectangle) = rectangle::Prepared::new(request, self.context.scale_factor)
            && let Some(bounds) = rectangle.geometry.intersection(bounds)
        {
            self.context.commands.push_rectangle(rectangle, bounds, clip);
        }
    }

    fn prepare_box_shadow(&mut self, shadow: &BoxShadow, bounds: PhysicalRect, clip: u16) {
        let Some(request) =
            self.context.shadows.prepare(&mut self.context.images, shadow, self.context.scale_factor)
        else {
            return;
        };
        match request {
            shadow::Prepared::Rectangle(rectangle) => self.prepare_rectangle(&rectangle, bounds, clip),
            shadow::Prepared::Image(request) => {
                let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
                if let Some(texture) = self.context.images.get(image) {
                    image::prepare(&request, &texture.data, bounds, 1.0, |image, bounds| {
                        self.context.commands.push_image(
                            image,
                            bounds,
                            clip,
                            texture.opaque,
                            texture.has_opaque_spans,
                        )
                    });
                }
            }
        }
    }

    fn prepare_image(&mut self, request: &ImageRequest, bounds: PhysicalRect, clip: u16) {
        let image = RendererImageId::from(KeyData::from_ffi(request.image.0));
        if let Some(texture) = self.context.images.get(image) {
            image::prepare(request, &texture.data, bounds, self.context.scale_factor, |image, bounds| {
                self.context.commands.push_image(
                    image,
                    bounds,
                    clip,
                    texture.opaque,
                    texture.has_opaque_spans,
                )
            });
        }
    }

    fn prepare_text(
        &mut self,
        request: &TextRequest,
        bounds: PhysicalRect,
        clip: u16,
    ) -> Option<PhysicalRect> {
        let area = request.area.to_physical(self.context.scale_factor);
        let visible_area = area.intersection(bounds)?;
        let string = RendererStringId::from(KeyData::from_ffi(request.text.0));
        let text = self.context.strings.get(string).expect("invalid StringHandle").data.as_ref();
        let (paragraph, paragraph_bounds) =
            self.context.text.prepare(request, text, self.context.scale_factor);
        let bounds = paragraph_bounds.intersection(visible_area)?;
        self.context.commands.push_text(PreparedText { paragraph, area, color: request.color }, bounds, clip);
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

    pub fn create_string(&mut self, string: StringData) -> StringId {
        let string = self.context.strings.insert(StoredString { data: string, live: true });
        StringId(string.data().as_ffi())
    }

    pub fn drop_string(&mut self, string: StringId) {
        let string = RendererStringId::from(KeyData::from_ffi(string.0));
        if let Some(string) = self.context.strings.get_mut(string) {
            string.live = false;
            self.context.has_dead_strings = true;
        }
    }

    pub fn string(&self, string: StringId) -> &str {
        let string = RendererStringId::from(KeyData::from_ffi(string.0));
        self.context.strings.get(string).expect("invalid StringHandle").data.as_ref()
    }

    pub fn text_offset_at_position(&mut self, request: &TextRequest, position: LogicalPoint) -> usize {
        let string = RendererStringId::from(KeyData::from_ffi(request.text.0));
        let text = self.context.strings.get(string).expect("invalid StringHandle").data.as_ref();
        self.context.text.offset_at_position(request, text, position, self.context.scale_factor)
    }

    pub fn measure_text(&mut self, request: &TextRequest) -> LogicalSize {
        let string = RendererStringId::from(KeyData::from_ffi(request.text.0));
        let text = self.context.strings.get(string).expect("invalid StringHandle").data.as_ref();
        self.context.text.measure(request, text, self.context.scale_factor)
    }

    pub fn measure_text_height(&mut self, request: &TextRequest) -> f32 {
        let string = RendererStringId::from(KeyData::from_ffi(request.text.0));
        let text = self.context.strings.get(string).expect("invalid StringHandle").data.as_ref();
        self.context.text.measure_height(request, text, self.context.scale_factor)
    }

    pub fn text_cursor_rect(&mut self, request: &TextRequest, byte_offset: usize) -> LogicalRect {
        let string = RendererStringId::from(KeyData::from_ffi(request.text.0));
        let text = self.context.strings.get(string).expect("invalid StringHandle").data.as_ref();
        self.context.text.cursor_rect(request, text, byte_offset, self.context.scale_factor)
    }
}

use pixel::PixelSpan;
use slotmap::{new_key_type, Key, KeyData, SlotMap};
use text::TextRenderer;

new_key_type! {
    pub struct RendererImageId;
    pub struct RendererStringId;
}

#[doc(hidden)]
pub struct RenderContext<B: PixelBuffer> {
    buffer: B,
    scale_factor: f32,
    images: SlotMap<RendererImageId, StoredImage>,
    has_dead_images: bool,
    strings: SlotMap<RendererStringId, StoredString>,
    has_dead_strings: bool,
    shadows: shadow::Cache,
    text: TextRenderer,
    commands: CommandList,
    clips: ClipStack,
}

struct StoredString {
    data: StringData,
    live: bool,
}

pub struct StoredImage {
    data: ImageData,
    alpha_rows: Box<[AlphaRow]>,
    has_opaque_spans: bool,
    opaque: bool,
    live: bool,
}

impl StoredImage {
    fn new(data: ImageData) -> Self {
        let width = data.texture_rect.width as usize;
        let height = data.texture_rect.height as usize;
        let bytes = data.pixels.bytes();
        let mut has_opaque_spans = false;
        let mut opaque = true;
        let rgba_opaque = || {
            (0..height).all(|line| {
                bytes[line * data.stride_bytes..][..width * 4].chunks_exact(4).all(|pixel| pixel[3] == 255)
            })
        };
        let alpha_rows = match data.format {
            blit::resource::ImageFormat::Rgb8 => Box::default(),
            blit::resource::ImageFormat::Rgba8 => {
                opaque = rgba_opaque();
                Box::default()
            }
            blit::resource::ImageFormat::Rgba8Premultiplied if width > u16::MAX as usize => {
                opaque = rgba_opaque();
                Box::default()
            }
            blit::resource::ImageFormat::Rgba8Premultiplied => {
                let mut rows = Vec::with_capacity(height);
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
                    opaque &= opaque_start == 0 && opaque_end == width;
                    rows.push(AlphaRow {
                        visible_start: visible_start as u16,
                        visible_end: visible_end as u16,
                        opaque_start: opaque_start as u16,
                        opaque_end: opaque_end as u16,
                    });
                }
                rows.into_boxed_slice()
            }
            blit::resource::ImageFormat::Alpha8(_) if width > u16::MAX as usize => {
                opaque = (0..height)
                    .all(|line| bytes[line * data.stride_bytes..][..width].iter().all(|alpha| *alpha == 255));
                Box::default()
            }
            blit::resource::ImageFormat::Alpha8(_) => {
                let mut rows = Vec::with_capacity(height);
                for y in 0..height {
                    let row = &bytes[y * data.stride_bytes..][..width];
                    let mut visible_start = width;
                    let mut visible_end = 0;
                    for (x, alpha) in row.iter().enumerate() {
                        if *alpha != 0 {
                            visible_start = visible_start.min(x);
                            visible_end = x + 1;
                        }
                        opaque &= *alpha == 255;
                    }
                    visible_start = visible_start.min(visible_end);
                    rows.push(AlphaRow {
                        visible_start: visible_start as u16,
                        visible_end: visible_end as u16,
                        opaque_start: 0,
                        opaque_end: 0,
                    });
                }
                rows.into_boxed_slice()
            }
        };
        Self { data, alpha_rows, has_opaque_spans, opaque, live: true }
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
        if self.has_dead_strings {
            let RenderContext { strings, text, .. } = self;
            strings.retain(|_, string| string.live);
            text.retain_strings(|string| {
                strings.contains_key(RendererStringId::from(KeyData::from_ffi(string.0)))
            });
            self.has_dead_strings = false;
        }
    }
}

#[cfg(test)]
mod test;
