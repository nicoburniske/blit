use blit::{Clip, FrameInfo, LogicalPoint, LogicalRect, PhysicalRect, Platform, Scale2, Size};
use blit_cpu::{
    Renderer, Scanline,
    command_list::{ClipId, CommandList},
    image::{ImageData, ImageHandle},
    text_types::{TextLayoutRequest, TextRequest, TextRunId, TextStyle},
};
use blit_diff::{Change, Myers, Reconciliation};

use crate::pixel::DesktopBuffer;

pub struct DesktopPlatform {
    renderer: Renderer<DesktopBuffer, Scanline>,
    current: CommandList,
    previous: CommandList,
    diff: Myers,
    damage: Vec<PhysicalRect>,
    previous_damage: Vec<PhysicalRect>,
    clip: ClipId,
    clips: Vec<ClipId>,
    invalidated: bool,
}

impl DesktopPlatform {
    pub(crate) fn new(renderer: Renderer<DesktopBuffer, Scanline>) -> Self {
        Self {
            renderer,
            current: CommandList::default(),
            previous: CommandList::default(),
            diff: Myers::default(),
            damage: Vec::new(),
            previous_damage: Vec::new(),
            clip: ClipId::default(),
            clips: Vec::new(),
            invalidated: true,
        }
    }

    pub(crate) fn renderer_mut(&mut self) -> &mut Renderer<DesktopBuffer, Scanline> {
        &mut self.renderer
    }

    pub(crate) fn set_device_scale(&mut self, scale: f32) {
        self.renderer.set_device_scale(scale);
    }

    pub(crate) fn invalidate_all(&mut self) {
        self.invalidated = true;
        self.previous_damage.clear();
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        self.renderer.create_image(data)
    }

    pub fn text_run(&mut self, text: &str, style: TextStyle) -> TextRunId {
        self.renderer.text_run(text, style)
    }

    pub fn text_offset_at_position(
        &mut self,
        request: &TextRequest,
        position: LogicalPoint,
    ) -> usize {
        self.renderer.text_offset_at_position(request, position)
    }

    pub fn text_cursor_rect(&mut self, request: &TextRequest, offset: usize) -> LogicalRect {
        self.renderer.text_cursor_rect(request, offset)
    }

    pub(crate) fn measure_text(&mut self, request: &TextLayoutRequest) -> Size {
        self.renderer.measure_text(request)
    }

    pub(crate) fn commands(&mut self) -> (&mut CommandList, ClipId, Scale2) {
        (
            &mut self.current,
            self.clip,
            Scale2::uniform(self.renderer.device_scale()),
        )
    }

    fn reconcile(&mut self) {
        self.damage.clear();
        if std::mem::take(&mut self.invalidated) {
            self.damage.push(self.renderer.screen());
        } else {
            match self
                .diff
                .reconcile(self.previous.len(), self.current.len(), |old, new| {
                    self.previous.equivalent(old, &self.current, new)
                }) {
                Reconciliation::Exact(changes) => {
                    for change in changes.iter().copied() {
                        let bounds = match change {
                            Change::Remove(index) => self.previous.get(index).bounds,
                            Change::Insert(index) => self.current.get(index).bounds,
                        };
                        push_damage(&mut self.damage, bounds);
                    }
                }
                Reconciliation::LimitExceeded { old, new } => {
                    let paired = old.len().min(new.len());
                    for offset in 0..paired {
                        let old = old.start + offset;
                        let new = new.start + offset;
                        if !self.previous.equivalent(old, &self.current, new) {
                            push_damage(&mut self.damage, self.previous.get(old).bounds);
                            let bounds = self.current.get(new).bounds;
                            if bounds != self.previous.get(old).bounds {
                                push_damage(&mut self.damage, bounds);
                            }
                        }
                    }
                    for index in old.start + paired..old.end {
                        push_damage(&mut self.damage, self.previous.get(index).bounds);
                    }
                    for index in new.start + paired..new.end {
                        push_damage(&mut self.damage, self.current.get(index).bounds);
                    }
                }
            }
        }
        let current_damage = self.damage.len();
        self.damage.extend_from_slice(&self.previous_damage);
        self.renderer.render(&self.current, &self.damage);
        self.previous_damage.clear();
        self.previous_damage
            .extend_from_slice(&self.damage[..current_damage]);
        std::mem::swap(&mut self.current, &mut self.previous);
    }
}

impl Platform for DesktopPlatform {
    fn begin(&mut self, _: FrameInfo) {
        self.current.clear();
        self.clip = ClipId::default();
        self.clips.clear();
    }

    fn end(&mut self) {
        self.reconcile();
    }
}

#[derive(Clone, Copy)]
pub struct BoundsClip;

fn push_damage(damage: &mut Vec<PhysicalRect>, bounds: PhysicalRect) {
    if bounds.width <= 0 || bounds.height <= 0 {
        return;
    }
    const MAX_DAMAGE: usize = 32;
    if damage.len() < MAX_DAMAGE {
        damage.push(bounds);
        return;
    }
    let len = damage.len();
    for index in 0..len / 2 {
        damage[index] = damage[index * 2].union(damage[index * 2 + 1]);
    }
    if len % 2 == 1 {
        damage[len / 2] = damage[len - 1];
    }
    damage.truncate(len.div_ceil(2));
    damage.push(bounds);
}

impl Clip<DesktopPlatform> for BoundsClip {
    fn push(&self, platform: &mut DesktopPlatform, area: LogicalRect) {
        let previous = platform.clip;
        platform.clip = platform
            .current
            .push_clip(previous, area, Default::default());
        platform.clips.push(previous);
    }

    fn pop(&self, platform: &mut DesktopPlatform) {
        platform.clip = platform.clips.pop().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::Rectangle;
    use blit::{Frame, Size};
    use blit_cpu::{
        Font, FontFace, RendererConfig, color::Color, style::Style, text_types::FontId,
    };

    #[test]
    fn frame_records_and_renders_platform_leaves() {
        let mut pixels = vec![0; 16];
        let mut renderer = Renderer::new(
            DesktopBuffer::new(4, 4),
            RendererConfig {
                fonts: vec![FontFace {
                    id: FontId::default(),
                    weight: 400,
                    font: Font::from_static(include_bytes!(env!("BLIT_TEST_FONT"))).unwrap(),
                }],
                font_metric_cache_capacity: 0,
                glyph_cache_capacity: 0,
                paragraph_cache_capacity: 0,
                shadow_cache_capacity: 0,
            },
        )
        .strategy(Scanline::default());
        renderer.buffer_mut().set(&mut pixels);
        let mut platform = DesktopPlatform::new(renderer);
        let mut frame = Frame::default();
        frame.render(
            &mut platform,
            FrameInfo::new(Size::new(4.0, 4.0)),
            |mut ui| {
                ui.add(Rectangle::new(Style::new().background(Color::WHITE)));
            },
        );
        assert!(pixels.iter().all(|pixel| *pixel != 0));
    }
}
