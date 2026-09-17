use std::time::Instant;

use blit::{Clip, FrameStage, LogicalPoint, LogicalRect, Platform, Size};
use blit_graphics::{
    GuiContext,
    image::{ImageData, ImageHandle, ImageRequest},
    scene::{BoxShadow, Rectangle, TextPalette},
    text::{Span, TextLayoutRequest, TextRequest, TextRunId, TextStyle},
};
use blit_std::widget::performance::{FrameProfiler, Profiled};

pub struct DesktopPlatform {
    gui: GuiContext,
    profiler: FrameProfiler,
    clock: Instant,
}

impl DesktopPlatform {
    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        self.gui.create_image(data)
    }

    pub fn text_run(&mut self, text: &str, style: TextStyle) -> TextRunId {
        self.gui.text_run(text, style)
    }

    pub fn measure_text(&mut self, request: &TextLayoutRequest) -> Size {
        self.gui.measure_text(request)
    }

    pub fn text_offset_at_position(
        &mut self,
        request: &TextRequest,
        position: LogicalPoint,
    ) -> usize {
        self.gui.text_offset_at_position(request, position)
    }

    pub fn text_cursor_rect(&mut self, request: &TextRequest, offset: usize) -> LogicalRect {
        self.gui.text_cursor_rect(request, offset)
    }

    pub fn paint_rectangle(&mut self, rectangle: Rectangle<'_>) {
        self.gui.paint_rectangle(rectangle)
    }

    pub fn paint_text(&mut self, text: TextRequest) {
        self.gui.paint_text(text)
    }

    pub fn paint_image(&mut self, image: ImageRequest) {
        self.gui.paint_image(image)
    }

    pub fn paint_shadow(&mut self, shadow: BoxShadow) {
        self.gui.paint_shadow(shadow)
    }
}

impl DesktopPlatform {
    pub(crate) fn new(gui: GuiContext) -> Self {
        Self {
            gui,
            profiler: FrameProfiler::default(),
            clock: Instant::now(),
        }
    }

    pub(crate) fn rich_text(
        &mut self,
        spans: &[Span<'_>],
        style: TextStyle,
    ) -> (TextRunId, TextPalette) {
        self.gui.rich_text(spans, style)
    }

    pub(crate) fn paint_text_palette(&mut self, text: TextRequest, palette: TextPalette) {
        self.gui.paint_text_palette(text, palette)
    }

    pub(crate) fn set_scale(&mut self, scale: f32) {
        self.gui.set_scale(scale)
    }

    pub(crate) fn gui_mut(&mut self) -> &mut GuiContext {
        &mut self.gui
    }
}

impl Profiled for DesktopPlatform {
    fn profiler(&self) -> &FrameProfiler {
        &self.profiler
    }
}

impl Platform for DesktopPlatform {
    fn frame_stage(&mut self, stage: FrameStage) {
        self.gui.frame_stage(stage);
        self.profiler.begin_stage(stage, self.clock.elapsed());
    }
}

#[derive(Clone, Copy)]
pub struct BoundsClip;

impl Clip<DesktopPlatform> for BoundsClip {
    fn push(&self, platform: &mut DesktopPlatform, area: LogicalRect) {
        blit_graphics::BoundsClip.push(&mut platform.gui, area)
    }

    fn pop(&self, platform: &mut DesktopPlatform) {
        blit_graphics::BoundsClip.pop(&mut platform.gui)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{RichText, Span};
    use crate::{atom::Rectangle, graphics_renderer::CpuGraphicsRenderer, pixel::DesktopBuffer};
    use blit::{Frame, FrameInfo, Sides, Size};
    use blit_cpu::{Renderer, RendererConfig, Scanline};
    use blit_graphics::{FontData, FontFamily, TextConfig, TextSystem, color::Color, text::FontId};
    use blit_std::layout::flex;

    #[test]
    fn nested_content_renders_at_device_scale() {
        let mut pixels = vec![0; 16 * 16];
        let text = TextSystem::new(
            TextConfig {
                fonts: vec![FontFamily {
                    id: FontId::default(),
                    fonts: vec![FontData::Static(include_bytes!(env!("BLIT_TEST_FONT")))],
                }],
                text_cache_capacity: 0,
                layout_cache_capacity: 0,
            },
            Box::new(blit_text_cosmic::Backend::without_system_fonts()),
        )
        .unwrap();
        let renderer = Renderer::new(
            DesktopBuffer::new(16, 16),
            RendererConfig {
                paint_cache_capacity: 0,
                glyph_cache_capacity: 0,
                shadow_cache_capacity: 0,
            },
        )
        .strategy(Scanline::default());
        let mut renderer = CpuGraphicsRenderer::new(renderer);
        renderer.buffer_mut().set(&mut pixels);
        renderer.set_scale(2.0);
        let mut platform = DesktopPlatform::new(GuiContext::new(text));
        platform.set_scale(2.0);
        let mut frame = Frame::default();
        frame.render(
            &mut platform,
            FrameInfo::new(Size::uniform(8.0)),
            |ui: crate::Ui<'_>| {
                let mut root = ui.layout(flex::column().padding(Sides::all(3.0)));
                root.insert(Rectangle::new().background(Color::from_rgba8(20, 24, 32, 255)));
                root.child(flex::item().fixed(2.0, 2.0))
                    .insert(Rectangle::new().background(Color::from_rgba8(70, 110, 220, 255)));
                root.insert(RichText::new(&[Span::new("").color(Color::WHITE)]));
            },
        );
        renderer.render(platform.gui_mut());
        assert_eq!(pixels[7 * 16 + 7], 0x0046_6edc);
        assert_eq!(pixels[14 * 16 + 14], 0x0014_1820);
    }
}
