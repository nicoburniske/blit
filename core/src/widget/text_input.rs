use unicode_segmentation::UnicodeSegmentation;

use super::Widget;
use crate::{
    Appearance, Clip, Content, Element, Layout, Sizing, TextCaret, TextContent, TextSelection, Ui,
    color::Color,
    geometry::{LogicalInsets, LogicalRect},
    input::{Input, Key},
    interact::{Sense, WidgetId},
    keyboard::{KeyboardKind, KeyboardRequest},
    paint::{BorderRadius, TextOptions, TextOverflow, TextRequest, TextStyle, TextWrap},
    resource::StringHandle,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextInputResponse {
    pub edited: bool,
    pub accepted: bool,
}

crate::widget! {
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
        pub preferred_width: f32 = 160.0,
        pub read_only: bool,
        pub keyboard_kind: KeyboardKind,
        pub request_caps: bool,
        // todo: don't love this being in here...
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
    display: Option<StringHandle>,
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
            display: None,
        }
    }
}

impl Widget for TextInput<'_> {
    type Output = TextInputResponse;

    fn build(mut self, ui: &mut Ui) -> TextInputResponse {
        self.state.cursor = self.state.cursor.min(self.state.text.len());
        while !self.state.text.is_char_boundary(self.state.cursor) {
            self.state.cursor -= 1;
        }
        self.state.anchor = self.state.anchor.min(self.state.text.len());
        while !self.state.text.is_char_boundary(self.state.anchor) {
            self.state.anchor -= 1;
        }
        self.update_password_mask();
        self.sync_display(ui);

        let id = self.state.id;
        let text_id = id.child("text");
        let previous_text_area = ui.geometry(text_id).map(|geometry| geometry.area);
        let focused = ui.is_focused(id);
        self.state.focused = focused;
        let mut input = ui.element(
            Element::new(
                Layout::horizontal()
                    .width(Sizing::grow().max(self.preferred_width))
                    .height(Sizing::fit().min(self.border_width * 2.0))
                    .padding(self.padding),
            )
            .appearance(
                Appearance::new()
                    .background(if focused {
                        self.focused_background
                    } else {
                        self.background
                    })
                    .border(
                        self.border_width,
                        if focused {
                            self.focused_border_color
                        } else {
                            self.border_color
                        },
                    )
                    .radius(self.radius)
                    .opacity(self.opacity),
            )
            .interact(id, Sense::FOCUS),
        );
        let interaction = input.interaction();
        let mut response = TextInputResponse::default();

        if interaction.pressed
            && let (Some(position), Some(area)) = (input.pointer_position(), previous_text_area)
        {
            let offset = input
                .platform()
                .text_offset_at_position(&self.request(area), position);
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
        }

        match *input.input() {
            Input::Text(character) if focused && !self.read_only && !character.is_control() => {
                self.delete_selection();
                self.state.text.insert(self.state.cursor, character);
                self.state.cursor += character.len_utf8();
                self.state.anchor = self.state.cursor;
                response.edited = true;
            }
            Input::Key(key)
                if key.pressed && key.key == Key::Backspace && focused && !self.read_only =>
            {
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
            Input::Key(key)
                if key.pressed && key.key == Key::Delete && focused && !self.read_only =>
            {
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
            Input::Key(key) if key.pressed && key.key == Key::ArrowLeft && focused => {
                self.state.cursor =
                    if !key.modifiers.shift() && self.state.cursor != self.state.anchor {
                        self.state.cursor.min(self.state.anchor)
                    } else {
                        self.state.text[..self.state.cursor]
                            .grapheme_indices(true)
                            .next_back()
                            .map_or(0, |(offset, _)| offset)
                    };
                if !key.modifiers.shift() {
                    self.state.anchor = self.state.cursor;
                }
            }
            Input::Key(key) if key.pressed && key.key == Key::ArrowRight && focused => {
                self.state.cursor =
                    if !key.modifiers.shift() && self.state.cursor != self.state.anchor {
                        self.state.cursor.max(self.state.anchor)
                    } else {
                        self.state.cursor
                            + self.state.text[self.state.cursor..]
                                .graphemes(true)
                                .next()
                                .map_or(0, str::len)
                    };
                if !key.modifiers.shift() {
                    self.state.anchor = self.state.cursor;
                }
            }
            Input::Key(key) if key.pressed && key.key == Key::Home && focused => {
                self.state.cursor = 0;
                if !key.modifiers.shift() {
                    self.state.anchor = self.state.cursor;
                }
            }
            Input::Key(key) if key.pressed && key.key == Key::End && focused => {
                self.state.cursor = self.state.text.len();
                if !key.modifiers.shift() {
                    self.state.anchor = self.state.cursor;
                }
            }
            Input::Key(key) if key.pressed && key.key == Key::Enter && focused => {
                response.accepted = true
            }
            _ => {}
        }
        if response.edited {
            self.update_password_mask();
            self.sync_display(&mut input);
        }

        if let Some(area) = previous_text_area {
            let cursor = input
                .platform()
                .text_cursor_rect(&self.request(area), self.display_offset(self.state.cursor));
            if cursor.x < area.x {
                self.state.scroll_x = (self.state.scroll_x - (area.x - cursor.x)).max(0.0);
            } else if cursor.x + self.cursor_width > area.x + area.width {
                self.state.scroll_x += cursor.x + self.cursor_width - area.x - area.width;
            }
        }

        let options = self.single_line_options();
        let selection = (self.state.cursor != self.state.anchor).then(|| TextSelection {
            start: self.display_offset(self.state.cursor.min(self.state.anchor)),
            end: self.display_offset(self.state.cursor.max(self.state.anchor)),
            color: self.selection_background,
        });
        let caret = focused.then(|| TextCaret {
            offset: self.display_offset(self.state.cursor),
            width: self.cursor_width,
            color: self.cursor_color,
        });
        drop(
            input.element(
                Element::new(Layout::horizontal().width(Sizing::grow()))
                    .id(text_id)
                    .clip(Clip::Bounds)
                    .content(Content::Text(TextContent {
                        text: self.state.display.as_ref().unwrap().into(),
                        color: self.text_color,
                        style: self.text_style,
                        options,
                        offset_x: self.state.scroll_x,
                        selection,
                        caret,
                    })),
            ),
        );

        if focused {
            input.platform().show_keyboard(&KeyboardRequest {
                kind: self.keyboard_kind,
                request_caps: self.request_caps,
                accept_button_text: self.accept_button_text,
                accept_button_enabled: self.accept_button_enabled,
                delete_button_enabled: self.delete_button_enabled && !self.state.text.is_empty(),
            });
        }
        response
    }
}

impl TextInput<'_> {
    fn request(&self, area: LogicalRect) -> TextRequest {
        TextRequest {
            text: self.state.display.as_ref().unwrap().into(),
            area,
            offset_x: self.state.scroll_x,
            color: self.text_color,
            style: self.text_style,
            options: self.single_line_options(),
        }
    }

    fn single_line_options(&self) -> TextOptions {
        TextOptions {
            wrap: TextWrap::None,
            overflow: TextOverflow::Clip,
            max_lines: Some(1),
            ..self.text_options
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

    fn sync_display(&mut self, ui: &mut Ui) {
        if self
            .state
            .display
            .as_deref()
            .is_none_or(|display| display != self.display_text())
        {
            let display = self.display_text().to_owned();
            self.state.display = Some(ui.platform().create_string(display));
        }
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
