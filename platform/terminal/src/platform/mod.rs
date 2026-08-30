pub mod draw;
pub mod widget;

use blit::{Clip, FrameInfo, LogicalPoint, LogicalRect, PhysicalRect, Platform, Size};
use blit_diff::{Change, Myers, Reconciliation};
use blit_term::{
    TerminalRenderer,
    command_list::{ClipId, CommandList},
    image::{ImageData, ImageHandle},
    text::{TextLayoutRequest, TextRequest, TextRunId},
};

pub struct TerminalPlatform {
    renderer: TerminalRenderer,
    current: CommandList,
    previous: CommandList,
    diff: Myers,
    damage: Vec<PhysicalRect>,
    clip: ClipId,
    clips: Vec<ClipId>,
    invalidated: bool,
}

impl TerminalPlatform {
    pub fn new(renderer: TerminalRenderer) -> Self {
        Self {
            renderer,
            current: CommandList::default(),
            previous: CommandList::default(),
            diff: Myers::default(),
            damage: Vec::new(),
            clip: ClipId::default(),
            clips: Vec::new(),
            invalidated: true,
        }
    }

    pub fn renderer(&self) -> &TerminalRenderer {
        &self.renderer
    }

    pub fn renderer_mut(&mut self) -> &mut TerminalRenderer {
        &mut self.renderer
    }

    pub fn invalidate_all(&mut self) {
        self.invalidated = true;
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        self.renderer.create_image(data)
    }

    pub fn text_run(&mut self, text: &str) -> TextRunId {
        self.renderer.text_run(text)
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

    pub fn measure_text(&mut self, request: &TextLayoutRequest) -> Size {
        self.renderer.measure_text(request)
    }
}

impl TerminalPlatform {
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
        self.renderer.render(&self.current, &self.damage);
        std::mem::swap(&mut self.current, &mut self.previous);
    }
}

impl Platform for TerminalPlatform {
    fn begin(&mut self, _: FrameInfo) {
        self.current.clear();
        self.clip = ClipId::default();
        self.clips.clear();
    }

    fn end(&mut self) {
        self.reconcile();
    }

    fn interaction_area(&self, area: LogicalRect, clip: LogicalRect) -> Option<LogicalRect> {
        self.renderer.interaction_area(area, clip)
    }
}

#[derive(Clone, Copy)]
pub struct BoundsClip;

impl Clip<TerminalPlatform> for BoundsClip {
    fn push(&self, platform: &mut TerminalPlatform, area: LogicalRect) {
        let previous = platform.clip;
        platform.clip = platform.current.push_clip(previous, area);
        platform.clips.push(previous);
    }

    fn pop(&self, platform: &mut TerminalPlatform) {
        platform.clip = platform.clips.pop().unwrap();
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::Block;
    use blit::{Frame, Size};
    use blit_term::{RendererConfig, color::Color};

    #[test]
    fn frame_records_and_renders_terminal_leaves() {
        let renderer = TerminalRenderer::new(RendererConfig::new().columns(4).rows(2));
        let mut platform = TerminalPlatform::new(renderer);
        let mut frame = Frame::default();
        frame.render(&mut platform, FrameInfo::new(Size::new(4.0, 2.0)), |ui| {
            ui.add(Block::new().background(Color::WHITE));
        });
        assert!(!platform.renderer().output().is_empty());
        frame.render(&mut platform, FrameInfo::new(Size::new(4.0, 2.0)), |ui| {
            ui.add(Block::new().background(Color::WHITE));
        });
        assert!(platform.renderer().output().is_empty());
    }
}
