use unicode_segmentation::UnicodeSegmentation;

use super::Widget;
use crate::{
    Ui,
    color::Color,
    container::{Item, Sizing},
    geometry::LogicalRect,
    input::{Input, Key},
    interact::{Sense, WidgetId},
    node::Content,
    text::{
        FontId, TextCaret, TextContent, TextOptions, TextOverflow, TextRequest, TextRunId,
        TextSelection, TextStyle, TextWrap,
    },
};

crate::builder! {
    pub struct TextInput<'a> {
        new(state: &'a mut TextInputState),
        text_color: Color = Color::BLACK,
        selection_background: Color = Color::GRAY,
        cursor_color: Color = Color::BLACK,
        cursor_width: f32 = 1.0,
        text_style: TextStyle = TextStyle::default(),
        text_options: TextOptions = TextOptions::default(),
        read_only: bool = false,
        mask: Option<char> = None,
    }
}

pub struct TextInputState {
    pub text: String,
    pub id: WidgetId,
    pub focused: bool,
    pub cursor: usize,
    pub anchor: usize,
    pub scroll_x: f32,
    mask_text: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextInputResponse {
    pub edited: bool,
    pub accepted: bool,
}

impl TextInput<'_> {
    pub fn style(mut self, style: impl Into<TextStyle>) -> Self {
        self.text_style = style.into();
        self
    }

    pub fn font(mut self, font: FontId) -> Self {
        self.text_style.font = font;
        self
    }

    pub fn text_size(mut self, size: f32) -> Self {
        self.text_style.size = size;
        self
    }

    pub fn text_weight(mut self, weight: u16) -> Self {
        self.text_style.weight = weight;
        self
    }
}

impl Default for TextInputState {
    fn default() -> Self {
        Self {
            text: String::new(),
            id: WidgetId::unique(),
            focused: false,
            cursor: 0,
            anchor: 0,
            scroll_x: 0.0,
            mask_text: String::new(),
        }
    }
}

impl Widget for TextInput<'_> {
    type Output = TextInputResponse;

    fn render(mut self, ui: &mut Ui) -> TextInputResponse {
        self.state.cursor = self.state.cursor.min(self.state.text.len());
        while !self.state.text.is_char_boundary(self.state.cursor) {
            self.state.cursor -= 1;
        }
        self.state.anchor = self.state.anchor.min(self.state.text.len());
        while !self.state.text.is_char_boundary(self.state.anchor) {
            self.state.anchor -= 1;
        }
        self.update_mask();
        let mut text_run = ui.text_run(self.display_text(), self.text_style);

        let id = self.state.id;
        let previous_area = ui.geometry(id);
        let focused = ui.is_focused(id);
        self.state.focused = focused;
        let interaction = ui.interact(id, Sense::FOCUS);
        let mut response = TextInputResponse::default();

        if interaction.pressed
            && let (Some(position), Some(area)) = (ui.pointer_position(), previous_area)
        {
            let offset = ui.text_offset_at_position(&self.request(text_run, area), position);
            self.state.cursor = if let Some(mask) = self.mask {
                self.state
                    .text
                    .char_indices()
                    .nth(offset / mask.len_utf8())
                    .map_or(self.state.text.len(), |(offset, _)| offset)
            } else {
                offset
            };
            self.state.anchor = self.state.cursor;
        }

        match *ui.input() {
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
            self.update_mask();
            text_run = ui.text_run(self.display_text(), self.text_style);
        }

        if let Some(area) = previous_area {
            let cursor = ui.text_cursor_rect(
                &self.request(text_run, area),
                self.display_offset(self.state.cursor),
            );
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
        let node = ui.add_leaf(
            Item {
                width: Sizing::grow(),
                height: Sizing::fit(),
            },
            Content::Text(TextContent {
                text: text_run,
                color: self.text_color,
                style: self.text_style,
                options,
                offset_x: self.state.scroll_x,
                selection,
                caret,
            }),
        );
        ui.set_node_id(node, id);

        response
    }
}

impl TextInput<'_> {
    fn request(&self, text: TextRunId, area: LogicalRect) -> TextRequest {
        TextRequest {
            text,
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

    fn update_mask(&mut self) {
        self.state.mask_text.clear();
        if let Some(mask) = self.mask {
            self.state
                .mask_text
                .extend(std::iter::repeat_n(mask, self.state.text.chars().count()));
        }
    }

    fn display_text(&self) -> &str {
        if self.mask.is_some() {
            &self.state.mask_text
        } else {
            &self.state.text
        }
    }

    fn display_offset(&self, source_offset: usize) -> usize {
        self.mask.map_or(source_offset, |mask| {
            self.state.text[..source_offset].chars().count() * mask.len_utf8()
        })
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
    fn mask_maps_utf8_offsets() {
        let mut state = TextInputState {
            text: "aé🙂".into(),
            ..TextInputState::default()
        };
        let mut input = TextInput::new(&mut state).mask(Some('●'));

        input.update_mask();

        assert_eq!(input.display_text(), "●●●");
        assert_eq!(input.display_offset("aé".len()), "●●".len());
    }
}
