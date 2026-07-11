mod font;
mod paragraph;

use bullseye::{LogicalPoint, LogicalRect, PhysicalRect, TextRequest};

use crate::{Pixel, PixelBuffer, RendererConfig};
use font::FontCache;
use paragraph::ParagraphCache;

pub struct TextRenderer {
    fonts: FontCache,
    paragraphs: ParagraphCache,
}

impl TextRenderer {
    pub fn new(config: RendererConfig) -> Self {
        Self {
            fonts: FontCache::new(config.fonts, config.glyph_cache_capacity),
            paragraphs: ParagraphCache::new(config.paragraph_cache_capacity),
        }
    }

    pub fn draw<B: PixelBuffer>(
        &mut self,
        buffer: &mut B,
        request: &TextRequest<'_>,
        clips: &[PhysicalRect],
        scale_factor: f32,
    ) {
        let paragraph = self.paragraphs.get(request, scale_factor, &mut self.fonts);
        let paragraph = match &paragraph {
            Ok(paragraph) => *paragraph,
            Err(paragraph) => paragraph,
        };
        if paragraph.width == 0 || paragraph.height == 0 {
            return;
        }
        let area = request.area.to_physical(scale_factor);
        let paragraph_rect = PhysicalRect {
            x: area.x + paragraph.x,
            y: area.y + paragraph.y,
            width: paragraph.width as i32,
            height: paragraph.height as i32,
        };
        let screen = PhysicalRect {
            x: 0,
            y: 0,
            width: buffer.width() as i32,
            height: buffer.height() as i32,
        };
        for clip in clips {
            let Some(clipped) = paragraph_rect
                .intersection(*clip)
                .and_then(|area| area.intersection(screen))
            else {
                continue;
            };
            let source_x = (clipped.x - paragraph_rect.x) as usize;
            for y in clipped.y..clipped.y + clipped.height {
                let source_y = (y - paragraph_rect.y) as usize;
                let alpha = &paragraph.alpha[source_y * paragraph.width + source_x
                    ..source_y * paragraph.width + source_x + clipped.width as usize];
                B::Pixel::blend_alpha_slice(
                    &mut buffer.line_mut(y as usize)[clipped.x as usize..]
                        [..clipped.width as usize],
                    request.color,
                    alpha,
                );
            }
        }
    }

    pub fn offset_at_position(
        &mut self,
        request: &TextRequest<'_>,
        position: LogicalPoint,
        scale_factor: f32,
    ) -> usize {
        let paragraph = self.paragraphs.get(request, scale_factor, &mut self.fonts);
        let paragraph = match &paragraph {
            Ok(paragraph) => *paragraph,
            Err(paragraph) => paragraph,
        };
        let x = (position.x - request.area.x) * scale_factor;
        let y = (position.y - request.area.y) * scale_factor;
        paragraph
            .carets
            .iter()
            .min_by(|left, right| {
                let left_distance = (left.x - x).powi(2) + (left.y + left.height / 2.0 - y).powi(2);
                let right_distance =
                    (right.x - x).powi(2) + (right.y + right.height / 2.0 - y).powi(2);
                left_distance.total_cmp(&right_distance)
            })
            .map_or(0, |caret| caret.byte_offset.min(request.text.len()))
    }

    pub fn cursor_rect(
        &mut self,
        request: &TextRequest<'_>,
        byte_offset: usize,
        scale_factor: f32,
    ) -> LogicalRect {
        let paragraph = self.paragraphs.get(request, scale_factor, &mut self.fonts);
        let paragraph = match &paragraph {
            Ok(paragraph) => *paragraph,
            Err(paragraph) => paragraph,
        };
        let Some(caret) = paragraph
            .carets
            .iter()
            .min_by_key(|caret| caret.byte_offset.abs_diff(byte_offset))
        else {
            return LogicalRect {
                x: request.area.x,
                y: request.area.y,
                width: 1.0,
                height: request.style.size,
            };
        };
        LogicalRect {
            x: request.area.x + caret.x / scale_factor,
            y: request.area.y + caret.y / scale_factor,
            width: 1.0,
            height: caret.height / scale_factor,
        }
    }
}
