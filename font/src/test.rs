use blit::paint::TextWrap;

use crate::{Font, Layout, LayoutSettings, Rasterizer};

static FONT: &[u8] = include_bytes!("../../resources/fonts/Montserrat-Regular.ttf");

#[test]
fn static_and_owned_fonts_match() {
    let static_font = Font::from_static(FONT).unwrap();
    let owned_font = Font::from_owned(FONT.to_vec().into_boxed_slice()).unwrap();
    let glyph = static_font.glyph_id('M');

    assert_eq!(glyph, owned_font.glyph_id('M'));
    assert_eq!(static_font.metrics(glyph, 24.0), owned_font.metrics(glyph, 24.0));
    assert_eq!(static_font.horizontal_line_metrics(24.0), owned_font.horizontal_line_metrics(24.0));
}

#[test]
fn layout_wraps_and_reuses_storage() {
    let font = Font::from_static(FONT).unwrap();
    let mut layout = Layout::default();
    let settings = LayoutSettings { max_width: Some(40.0), wrap: TextWrap::Word, ..Default::default() };
    layout.layout(&font, "one two three", 16.0, settings);
    assert!(layout.lines().unwrap().len() > 1);
    let allocated = layout.allocated_bytes();
    layout.layout(&font, "a", 16.0, settings);
    assert_eq!(layout.lines().unwrap().len(), 1);
    assert_eq!(layout.allocated_bytes(), allocated);
}

#[test]
fn rasterizer_reuses_storage() {
    let font = Font::from_static(FONT).unwrap();
    let mut rasterizer = Rasterizer::default();
    let (metrics, alpha) = rasterizer.rasterize(&font, font.glyph_id('M'), 24.0);
    assert_eq!(alpha.len(), metrics.width * metrics.height);
    assert!(alpha.iter().any(|alpha| *alpha != 0));
    let allocated = rasterizer.allocated_bytes();
    rasterizer.rasterize(&font, font.glyph_id('M'), 24.0);
    assert_eq!(rasterizer.allocated_bytes(), allocated);
}
