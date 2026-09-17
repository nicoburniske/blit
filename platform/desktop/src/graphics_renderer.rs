use blit::PhysicalRect;
use blit_cpu::{Renderer, Scanline};
use blit_diff::{Change, Myers, Reconciliation};
use blit_graphics::{GuiContext, scene::CommandList};

use crate::pixel::DesktopBuffer;

pub struct CpuGraphicsRenderer {
    renderer: Renderer<DesktopBuffer, Scanline>,
    previous: CommandList,
    diff: Myers,
    damage: Vec<PhysicalRect>,
    previous_damage: Vec<PhysicalRect>,
    invalidated: bool,
}

impl CpuGraphicsRenderer {
    pub fn new(renderer: Renderer<DesktopBuffer, Scanline>) -> Self {
        Self {
            renderer,
            previous: CommandList::default(),
            diff: Myers::default(),
            damage: Vec::new(),
            previous_damage: Vec::new(),
            invalidated: true,
        }
    }

    pub fn buffer_mut(&mut self) -> &mut DesktopBuffer {
        self.renderer.buffer_mut()
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.renderer.set_scale(blit::Scale2::uniform(scale));
        self.invalidate_all();
    }

    pub fn invalidate_all(&mut self) {
        self.invalidated = true;
        self.previous_damage.clear();
    }

    pub fn render(&mut self, gui: &mut GuiContext) {
        self.damage.clear();
        let frame = gui.render_input();
        if std::mem::take(&mut self.invalidated) {
            self.damage.push(self.renderer.screen());
        } else {
            match self
                .diff
                .reconcile(self.previous.len(), frame.scene.len(), |old, new| {
                    self.previous.equivalent(old, frame.scene, new)
                }) {
                Reconciliation::Exact(changes) => {
                    for change in changes.iter().copied() {
                        let bounds = match change {
                            Change::Remove(index) => self.previous.get(index).bounds,
                            Change::Insert(index) => frame.scene.get(index).bounds,
                        };
                        push_damage(&mut self.damage, bounds);
                    }
                }
                Reconciliation::LimitExceeded { old, new } => {
                    let paired = old.len().min(new.len());
                    for offset in 0..paired {
                        let old = old.start + offset;
                        let new = new.start + offset;
                        if !self.previous.equivalent(old, frame.scene, new) {
                            push_damage(&mut self.damage, self.previous.get(old).bounds);
                            let bounds = frame.scene.get(new).bounds;
                            if bounds != self.previous.get(old).bounds {
                                push_damage(&mut self.damage, bounds);
                            }
                        }
                    }
                    for index in old.start + paired..old.end {
                        push_damage(&mut self.damage, self.previous.get(index).bounds);
                    }
                    for index in new.start + paired..new.end {
                        push_damage(&mut self.damage, frame.scene.get(index).bounds);
                    }
                }
            }
        }
        let current_damage = self.damage.len();
        self.damage.extend_from_slice(&self.previous_damage);
        self.renderer
            .render(frame.text, frame.image_uploads, frame.scene, &self.damage);
        self.previous_damage.clear();
        self.previous_damage
            .extend_from_slice(&self.damage[..current_damage]);
        gui.finish_frame();
        let previous = std::mem::take(&mut self.previous);
        self.previous = gui.replace_scene(previous);
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
