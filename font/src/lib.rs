#![feature(portable_simd)]

mod layout;
mod raster;
#[cfg(test)]
mod test;

use std::{
    error::Error,
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
};

pub use layout::{CharacterData, GlyphPosition, GlyphRasterConfig, Layout, LayoutSettings, LinePosition};
pub use raster::Rasterizer;
use ttf_parser::{Face, FaceParsingError, Rect};

static NEXT_FONT_ID: AtomicUsize = AtomicUsize::new(0);

enum FontData {
    Static(&'static [u8]),
    Owned(Box<[u8]>),
}

pub struct Font {
    data: FontData,
    id: usize,
}

impl Font {
    pub fn from_static(data: &'static [u8]) -> Result<Self, InvalidFont> {
        Face::parse(data, 0).map_err(InvalidFont)?;
        Ok(Self { data: FontData::Static(data), id: NEXT_FONT_ID.fetch_add(1, Ordering::Relaxed) })
    }

    pub fn from_owned(data: Box<[u8]>) -> Result<Self, InvalidFont> {
        Face::parse(&data, 0).map_err(InvalidFont)?;
        Ok(Self { data: FontData::Owned(data), id: NEXT_FONT_ID.fetch_add(1, Ordering::Relaxed) })
    }

    pub fn glyph_id(&self, character: char) -> GlyphId {
        self.face().glyph_index(character).map_or(GlyphId(0), |id| GlyphId(id.0))
    }

    pub fn metrics(&self, glyph: GlyphId, size: f32) -> Metrics {
        Self::metrics_from_face(&self.face(), glyph, size)
    }

    pub fn horizontal_line_metrics(&self, size: f32) -> LineMetrics {
        Self::line_metrics_from_face(&self.face(), size)
    }

    fn face(&self) -> Face<'_> {
        let data = match &self.data {
            FontData::Static(data) => *data,
            FontData::Owned(data) => data.as_ref(),
        };
        match Face::parse(data, 0) {
            Ok(face) => face,
            Err(_) => unreachable!("validated immutable font became invalid"),
        }
    }

    fn id(&self) -> usize { self.id }

    fn metrics_from_face(face: &Face<'_>, glyph: GlyphId, size: f32) -> Metrics {
        Self::scale_metrics(Self::unscaled_metrics_from_face(face, glyph), face.units_per_em(), size)
    }

    fn unscaled_metrics_from_face(face: &Face<'_>, glyph: GlyphId) -> UnscaledMetrics {
        UnscaledMetrics {
            advance_width: face.glyph_hor_advance(ttf_parser::GlyphId(glyph.0)).unwrap_or(0),
            bounds: face.glyph_bounding_box(ttf_parser::GlyphId(glyph.0)),
        }
    }

    fn scale_metrics(metrics: UnscaledMetrics, units_per_em: u16, size: f32) -> Metrics {
        if !size.is_finite() || size <= 0.0 {
            return Metrics::default();
        }

        let scale = size / units_per_em as f32;
        let advance_width = metrics.advance_width as f32 * scale;
        let Some(bounds) = metrics.bounds else {
            return Metrics { advance_width, ..Metrics::default() };
        };
        let bounds = OutlineBounds {
            xmin: bounds.x_min as f32 * scale,
            ymin: bounds.y_min as f32 * scale,
            width: (bounds.x_max - bounds.x_min) as f32 * scale,
            height: (bounds.y_max - bounds.y_min) as f32 * scale,
        };
        let mut offset_x = bounds.xmin.fract();
        let mut offset_y = (1.0 - bounds.height.fract() - bounds.ymin.fract()).fract();
        if offset_x < 0.0 {
            offset_x += 1.0;
        }
        if offset_y < 0.0 {
            offset_y += 1.0;
        }
        Metrics {
            xmin: bounds.xmin.floor() as i32,
            ymin: bounds.ymin.floor() as i32,
            width: (bounds.width + offset_x).ceil().max(0.0) as usize,
            height: (bounds.height + offset_y).ceil().max(0.0) as usize,
            advance_width,
            bounds,
        }
    }

    fn line_metrics_from_face(face: &Face<'_>, size: f32) -> LineMetrics {
        if !size.is_finite() || size <= 0.0 {
            return LineMetrics::default();
        }
        let scale = size / face.units_per_em() as f32;
        let ascent = face.ascender() as f32 * scale;
        let descent = face.descender() as f32 * scale;
        let line_gap = face.line_gap() as f32 * scale;
        LineMetrics { ascent, descent, line_gap, new_line_size: ascent - descent + line_gap }
    }
}

#[derive(Clone, Copy)]
struct UnscaledMetrics {
    advance_width: u16,
    bounds: Option<Rect>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GlyphId(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OutlineBounds {
    pub xmin: f32,
    pub ymin: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    pub xmin: i32,
    pub ymin: i32,
    pub width: usize,
    pub height: usize,
    pub advance_width: f32,
    pub bounds: OutlineBounds,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub new_line_size: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidFont(FaceParsingError);

impl fmt::Display for InvalidFont {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid font: {:?}", self.0)
    }
}

impl Error for InvalidFont {}
