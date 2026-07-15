mod font;
mod paragraph;

use blit::{Color, LogicalPoint, LogicalRect, LogicalSize, PhysicalRect, TextRequest};

use crate::{Pixel, PixelSpan, RendererConfig};
use font::FontCache;
use paragraph::{Paragraph, ParagraphCache, ParagraphKey};

pub struct TextRenderer {
    fonts: FontCache,
    paragraphs: ParagraphCache,
    frame: Vec<PreparedParagraph>,
}

struct PreparedParagraph {
    key: ParagraphKey,
    paragraph: Paragraph,
}

impl TextRenderer {
    pub fn new(config: RendererConfig) -> Self {
        Self {
            fonts: FontCache::new(config.fonts, config.glyph_cache_capacity),
            paragraphs: ParagraphCache::new(config.paragraph_cache_capacity),
            frame: Vec::new(),
        }
    }

    pub fn prepare(
        &mut self,
        request: &TextRequest<'_>,
        scale_factor: f32,
    ) -> (usize, PhysicalRect) {
        let key = ParagraphCache::key(request, scale_factor);
        let index = match self
            .frame
            .iter()
            .position(|prepared| prepared.key == key && prepared.paragraph.matches(request.text))
        {
            Some(index) => index,
            None => {
                let (key, paragraph) = self.paragraphs.take(request, scale_factor, &mut self.fonts);
                let index = self.frame.len();
                self.frame.push(PreparedParagraph { key, paragraph });
                index
            }
        };
        let area = request.area.to_physical(scale_factor);
        let paragraph = &self.frame[index].paragraph;
        if paragraph.width == 0 || paragraph.height == 0 {
            return (index, PhysicalRect::default());
        }
        (
            index,
            PhysicalRect {
                x: area.x.saturating_add(paragraph.x),
                y: area.y.saturating_add(paragraph.y),
                width: paragraph.width as i32,
                height: paragraph.height as i32,
            },
        )
    }

    pub fn draw_line<P: Pixel>(
        &self,
        paragraph: usize,
        area: PhysicalRect,
        color: Color,
        line: i32,
        row: PixelSpan<'_, P>,
        clip: PhysicalRect,
    ) {
        let Some(paragraph) = self
            .frame
            .get(paragraph)
            .map(|prepared| &prepared.paragraph)
        else {
            return;
        };
        let paragraph_rect = PhysicalRect {
            x: area.x + paragraph.x,
            y: area.y + paragraph.y,
            width: paragraph.width as i32,
            height: paragraph.height as i32,
        };
        let line_rect = PhysicalRect {
            x: row.x,
            y: line,
            width: row.pixels.len() as i32,
            height: 1,
        };
        let Some(clipped) = paragraph_rect
            .intersection(clip)
            .and_then(|area| area.intersection(line_rect))
        else {
            return;
        };
        let source_x = (clipped.x - paragraph_rect.x) as usize;
        let source_y = (line - paragraph_rect.y) as usize;
        let alpha = &paragraph.alpha[source_y * paragraph.width + source_x
            ..source_y * paragraph.width + source_x + clipped.width as usize];
        P::blend_alpha_slice(
            &mut row.pixels[(clipped.x - row.x) as usize..][..clipped.width as usize],
            color,
            alpha,
        );
    }

    pub fn finish_frame(&mut self) {
        for prepared in self.frame.drain(..) {
            self.paragraphs.restore(prepared.key, prepared.paragraph);
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

    pub fn measure(&mut self, request: &TextRequest<'_>, scale_factor: f32) -> LogicalSize {
        let paragraph = self.paragraphs.get(request, scale_factor, &mut self.fonts);
        let paragraph = match &paragraph {
            Ok(paragraph) => *paragraph,
            Err(paragraph) => paragraph,
        };
        LogicalSize {
            width: paragraph.layout_width / scale_factor,
            height: paragraph.layout_height / scale_factor,
        }
    }

    pub fn measure_height(&mut self, request: &TextRequest<'_>, scale_factor: f32) -> f32 {
        self.paragraphs
            .measure_height(request, scale_factor, &mut self.fonts)
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
