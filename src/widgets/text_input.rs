use crate::{Color, Input, Rect, Ui};

#[derive(Debug, Default)]
pub struct TextInput {
    pub text: String,
    focused: bool,
}

impl TextInput {
    pub fn render(&mut self, ui: &mut Ui, area: Rect) {
        let old_focused = self.focused;
        let old_len = self.text.len();
        match ui.input().clone() {
            Input::PointerDown { x, y } => self.focused = area.contains(x, y),
            Input::Char(character) if self.focused => self.text.push(character),
            Input::Backspace if self.focused => {
                self.text.pop();
            }
            _ => {}
        }
        if self.focused != old_focused || self.text.len() != old_len {
            ui.invalidate(area);
        }
        ui.fill_rect(
            area,
            if self.focused {
                Color::from_rgba8(245, 245, 250, 255)
            } else {
                Color::from_rgba8(205, 210, 220, 255)
            },
        );
    }
}
