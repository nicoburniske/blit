use unicode_segmentation::UnicodeSegmentation;

use crate::{
    Color, Input, KeyboardKind, KeyboardRequest, LogicalInsets, LogicalRect, LogicalSize, Sense,
    SizedComponent, Text, TextOptions, TextOverflow, TextRequest, TextStyle, TextWrap, Ui,
    WidgetId,
    widgets::{BorderRadius, Rectangle},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextInputResponse {
    pub edited: bool,
    pub accepted: bool,
}

crate::component! {
    pub struct TextInput<'a> {
        new(state: &'a mut TextInputState);
        pub background: Color = Color::WHITE,
        pub focused_background: Color = Color::WHITE,
        pub border_color: Color = Color::GRAY,
        pub focused_border_color: Color = Color::BLACK,
        pub border_width: f32 = 1.0,
        pub radius: BorderRadius,
        pub opacity: f32 = 1.0,
        pub text_color: Color = Color::BLACK,
        pub selection_background: Color = Color::GRAY,
        pub cursor_color: Color = Color::BLACK,
        pub cursor_width: f32 = 1.0,
        pub text_style: TextStyle,
        pub text_options: TextOptions,
        pub padding: LogicalInsets = LogicalInsets::uniform(4.0),
        pub read_only: bool,
        pub keyboard_kind: KeyboardKind,
        pub request_caps: bool,
        pub accept_button_text: &'a str = "",
        pub accept_button_enabled: bool = true,
        pub delete_button_enabled: bool = true,
    }
    features: [padding, border, radius, text_style]
}

pub struct TextInputState {
    pub text: String,
    pub password_visible: bool,
    pub id: WidgetId,
    pub focused: bool,
    pub cursor: usize,
    pub anchor: usize,
    pub scroll_x: f32,
    pub password_mask: String,
}

impl Default for TextInputState {
    fn default() -> Self {
        Self {
            text: String::new(),
            password_visible: false,
            id: WidgetId::unique(),
            focused: false,
            cursor: 0,
            anchor: 0,
            scroll_x: 0.0,
            password_mask: String::new(),
        }
    }
}

impl TextInput<'_> {
    pub fn render(mut self, ui: &mut Ui, area: LogicalRect) -> TextInputResponse {
        self.state.cursor = self.state.cursor.min(self.state.text.len());
        while !self.state.text.is_char_boundary(self.state.cursor) {
            self.state.cursor -= 1;
        }
        self.state.anchor = self.state.anchor.min(self.state.text.len());
        while !self.state.text.is_char_boundary(self.state.anchor) {
            self.state.anchor -= 1;
        }
        self.update_password_mask();
        let inner = area.inset(self.padding);
        let mut response = TextInputResponse::default();
        let interaction = ui.interact(self.state.id, area, Sense::FOCUS);
        let focused = ui.is_focused(self.state.id);
        let mut changed = self.state.focused != focused;
        self.state.focused = focused;

        if interaction.pressed {
            if let Some(position) = ui.pointer_position() {
                let offset = ui
                    .platform()
                    .text_offset_at_position(&self.request(inner), position);
                self.state.cursor = if !self.password_masked() {
                    offset
                } else {
                    self.state
                        .text
                        .char_indices()
                        .nth(offset / '●'.len_utf8())
                        .map_or(self.state.text.len(), |(offset, _)| offset)
                };
                self.state.anchor = self.state.cursor;
                changed = true;
            }
        }

        match ui.input().clone() {
            Input::Char(character)
                if self.state.focused && !self.read_only && !character.is_control() =>
            {
                self.delete_selection();
                self.state.text.insert(self.state.cursor, character);
                self.state.cursor += character.len_utf8();
                self.state.anchor = self.state.cursor;
                response.edited = true;
            }
            Input::Backspace if self.state.focused && !self.read_only => {
                if self.delete_selection() {
                    response.edited = true;
                } else if self.state.cursor != 0 {
                    let previous = self.state.text[..self.state.cursor]
                        .grapheme_indices(true)
                        .next_back()
                        .map_or(0, |(offset, _)| offset);
                    self.state.text.drain(previous..self.state.cursor);
                    self.state.cursor = previous;
                    self.state.anchor = previous;
                    response.edited = true;
                }
            }
            Input::Delete if self.state.focused && !self.read_only => {
                if self.delete_selection() {
                    response.edited = true;
                } else if self.state.cursor < self.state.text.len() {
                    let next = self.state.cursor
                        + self.state.text[self.state.cursor..]
                            .graphemes(true)
                            .next()
                            .map_or(0, str::len);
                    self.state.text.drain(self.state.cursor..next);
                    self.state.anchor = self.state.cursor;
                    response.edited = true;
                }
            }
            Input::CursorLeft if self.state.focused => {
                self.state.cursor = if self.state.cursor != self.state.anchor {
                    self.state.cursor.min(self.state.anchor)
                } else {
                    self.state.text[..self.state.cursor]
                        .grapheme_indices(true)
                        .next_back()
                        .map_or(0, |(offset, _)| offset)
                };
                self.state.anchor = self.state.cursor;
                changed = true;
            }
            Input::CursorRight if self.state.focused => {
                self.state.cursor = if self.state.cursor != self.state.anchor {
                    self.state.cursor.max(self.state.anchor)
                } else {
                    self.state.cursor
                        + self.state.text[self.state.cursor..]
                            .graphemes(true)
                            .next()
                            .map_or(0, str::len)
                };
                self.state.anchor = self.state.cursor;
                changed = true;
            }
            Input::Enter if self.state.focused => response.accepted = true,
            _ => {}
        }
        if response.edited {
            self.update_password_mask();
        }

        let cursor_offset = self.display_offset(self.state.cursor);
        let cursor = ui
            .platform()
            .text_cursor_rect(&self.request(inner), cursor_offset);
        if cursor.x < inner.x {
            self.state.scroll_x = (self.state.scroll_x - (inner.x - cursor.x)).max(0.0);
        } else if cursor.x + self.cursor_width > inner.x + inner.width {
            self.state.scroll_x += cursor.x + self.cursor_width - inner.x - inner.width;
        }

        if response.edited || changed {
            ui.invalidate(area);
        }

        Rectangle::new(area)
            .background(if self.state.focused {
                self.focused_background
            } else {
                self.background
            })
            .border(
                self.border_width,
                if self.state.focused {
                    self.focused_border_color
                } else {
                    self.border_color
                },
            )
            .radius(self.radius)
            .opacity(self.opacity)
            .render(ui);

        let request = self.request(inner);
        if self.state.cursor != self.state.anchor {
            let start = ui.platform().text_cursor_rect(
                &request,
                self.display_offset(self.state.cursor.min(self.state.anchor)),
            );
            let end = ui.platform().text_cursor_rect(
                &request,
                self.display_offset(self.state.cursor.max(self.state.anchor)),
            );
            let left = start.x.max(inner.x);
            let right = end.x.min(inner.x + inner.width);
            let top = start.y.max(inner.y);
            let bottom = (start.y + start.height).min(inner.y + inner.height);
            if right > left && bottom > top {
                Rectangle::new(LogicalRect {
                    x: left,
                    y: top,
                    width: right - left,
                    height: bottom - top,
                })
                .background(self.selection_background)
                .render(ui);
            }
        }

        Text::new(self.display_text())
            .offset_x(self.state.scroll_x)
            .color(self.text_color)
            .font(self.text_style.font)
            .text_size(self.text_style.size)
            .text_weight(self.text_style.weight)
            .options(request.options)
            .render(ui, inner);

        if self.state.focused {
            let cursor = ui
                .platform()
                .text_cursor_rect(&request, self.display_offset(self.state.cursor));
            let x = cursor.x.clamp(
                inner.x,
                (inner.x + inner.width - self.cursor_width).max(inner.x),
            );
            let top = cursor.y.max(inner.y);
            let bottom = (cursor.y + cursor.height).min(inner.y + inner.height);
            if bottom > top {
                Rectangle::new(LogicalRect {
                    x,
                    y: top,
                    width: self.cursor_width.min(inner.width),
                    height: bottom - top,
                })
                .background(self.cursor_color)
                .render(ui);
            }
            ui.platform().show_keyboard(&KeyboardRequest {
                kind: self.keyboard_kind,
                request_caps: self.request_caps,
                accept_button_text: self.accept_button_text,
                accept_button_enabled: self.accept_button_enabled,
                delete_button_enabled: self.delete_button_enabled && !self.state.text.is_empty(),
            });
        }

        response
    }

    fn request(&self, area: LogicalRect) -> TextRequest<'_> {
        let mut options = self.text_options;
        options.wrap = TextWrap::None;
        options.overflow = TextOverflow::Clip;
        options.max_lines = Some(1);
        TextRequest {
            text: self.display_text(),
            area,
            offset_x: self.state.scroll_x,
            color: self.text_color,
            style: self.text_style,
            options,
            intrinsic_height: false,
        }
    }

    fn update_password_mask(&mut self) {
        if !self.password_masked() {
            self.state.password_mask.clear();
            return;
        }
        self.state.password_mask.clear();
        self.state
            .password_mask
            .extend(std::iter::repeat_n('●', self.state.text.chars().count()));
    }

    fn display_text(&self) -> &str {
        if self.password_masked() {
            &self.state.password_mask
        } else {
            &self.state.text
        }
    }

    fn display_offset(&self, source_offset: usize) -> usize {
        if self.password_masked() {
            self.state.text[..source_offset].chars().count() * '●'.len_utf8()
        } else {
            source_offset
        }
    }

    fn password_masked(&self) -> bool {
        self.keyboard_kind == KeyboardKind::Password && !self.state.password_visible
    }

    fn delete_selection(&mut self) -> bool {
        let selection =
            self.state.cursor.min(self.state.anchor)..self.state.cursor.max(self.state.anchor);
        if selection.is_empty() {
            return false;
        }
        self.state.cursor = selection.start;
        self.state.anchor = selection.start;
        self.state.text.drain(selection);
        true
    }
}

impl SizedComponent for TextInput<'_> {
    type Output = TextInputResponse;

    fn measure(&self, _: &mut Ui, available: LogicalRect) -> LogicalSize {
        let height = (self.text_style.size + self.padding.top + self.padding.bottom)
            .max(self.border_width * 2.0)
            .min(available.height);
        LogicalSize {
            width: available.width,
            height,
        }
    }

    fn render(self, ui: &mut Ui, area: LogicalRect) -> Self::Output {
        TextInput::render(self, ui, area)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_mask_uses_dots_and_maps_utf8_offsets() {
        let mut state = TextInputState {
            text: "aé🙂".into(),
            ..TextInputState::default()
        };
        let mut input = TextInput::new(&mut state).keyboard_kind(KeyboardKind::Password);

        input.update_password_mask();

        assert_eq!(input.display_text(), "●●●");
        assert_eq!(input.display_offset("aé".len()), "●●".len());

        input.state.password_visible = true;
        input.update_password_mask();
        assert_eq!(input.display_text(), "aé🙂");
    }
}
