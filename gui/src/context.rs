use blit::{Clip, LogicalPoint, LogicalRect, LogicalSize, Scale2};

use crate::{
    Context, TextSystem,
    display_list::{BoxShadow, ClipId, DisplayList, Mesh, Rectangle, TextPalette},
    image::ImageRequest,
    text::{Span, TextLayoutRequest, TextRequest, TextRunId, TextSource, TextStyle},
};

pub struct GuiContext {
    text: TextSystem,
    scale: Scale2,
    display_list: DisplayList,
    clip: ClipId,
    clips: Vec<ClipId>,
}

impl AsMut<GuiContext> for GuiContext {
    fn as_mut(&mut self) -> &mut GuiContext {
        self
    }
}

impl blit::Context for GuiContext {
    type Scalar = f32;
}

/// data needed to render one frame
#[non_exhaustive]
pub struct RenderInput<'a> {
    pub display_list: &'a mut DisplayList,
    pub text: &'a mut TextSystem,
    pub scale: Scale2,
}

impl GuiContext {
    pub fn new(text: TextSystem) -> Self {
        Self {
            text,
            scale: Scale2::IDENTITY,
            display_list: DisplayList::default(),
            clip: ClipId::default(),
            clips: Vec::new(),
        }
    }

    pub fn text_run<'a>(&mut self, text: impl Into<TextSource<'a>>, style: TextStyle) -> TextRunId {
        self.text.text_run(text.into(), style)
    }

    pub fn rich_text(&mut self, text: &str, spans: &[Span], style: TextStyle) -> (TextRunId, TextPalette) {
        (
            self.text.rich_text(text, spans, style),
            self.display_list.text_palette(spans),
        )
    }

    pub fn measure_text(&mut self, request: &TextLayoutRequest) -> LogicalSize {
        self.text.measure(request)
    }

    pub fn text_offset_at_position(&mut self, request: &TextRequest, position: LogicalPoint) -> usize {
        self.text.offset_at_position(request, position)
    }

    pub fn text_cursor_rect(&mut self, request: &TextRequest, offset: usize) -> LogicalRect {
        self.text.cursor_rect(request, offset, self.scale.x.recip())
    }

    pub fn paint_rectangle(&mut self, rectangle: Rectangle<'_>) {
        let bounds = rectangle.area.to_physical(self.scale);
        self.display_list.push_rectangle(rectangle, bounds, self.clip);
    }

    pub fn paint_text(&mut self, text: TextRequest) {
        self.paint_text_palette(text, TextPalette::NONE);
    }

    pub fn paint_text_palette(&mut self, text: TextRequest, palette: TextPalette) {
        let bounds = text.area.to_physical(self.scale);
        self.display_list.push_text_palette(text, palette, bounds, self.clip);
    }

    pub fn paint_image(&mut self, image: ImageRequest) {
        let bounds = image.area.to_physical(self.scale);
        self.display_list.push_image(image, bounds, self.clip);
    }

    pub fn paint_shadow(&mut self, shadow: BoxShadow) {
        let bounds = shadow.bounds().to_physical(self.scale);
        self.display_list.push_box_shadow(shadow, bounds, self.clip);
    }

    pub fn paint_mesh(&mut self, mesh: Mesh<'_>) {
        self.display_list.push_mesh(mesh, self.scale, self.clip);
    }

    pub fn set_scale(&mut self, scale: f32) {
        assert!(scale.is_finite() && scale > 0.0);
        self.scale = Scale2::uniform(scale);
    }

    pub fn render_input(&mut self) -> RenderInput<'_> {
        RenderInput {
            display_list: &mut self.display_list,
            text: &mut self.text,
            scale: self.scale,
        }
    }

    pub fn finish_frame(&mut self) {
        self.display_list.clear();
        self.clip = ClipId::default();
        self.clips.clear();
        self.text.finish_frame();
    }
}

#[derive(Clone, Copy)]
pub struct BoundsClip;

impl<C: Context> Clip<C> for BoundsClip {
    fn push(&self, context: &mut C, area: LogicalRect) {
        let gui = context.as_mut();
        let previous = gui.clip;
        gui.clip = gui.display_list.push_clip(previous, area, Default::default());
        gui.clips.push(previous);
    }

    fn pop(&self, context: &mut C) {
        let gui = context.as_mut();
        gui.clip = gui.clips.pop().unwrap();
    }
}
