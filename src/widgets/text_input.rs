use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::{
    Color, FontId, FontWeight, Input, KeyboardKind, KeyboardRequest, LogicalInsets, LogicalRect,
    Text, TextOptions, TextOverflow, TextRequest, TextStyle, TextWrap, Ui,
    widgets::{BorderRadius, Rectangle},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextInputResponse {
    pub edited: bool,
    pub accepted: bool,
}

#[derive(Debug)]
pub struct TextInput {
    pub text: String,
    pub focused: bool,
    pub cursor: usize,
    pub anchor: usize,
    pub scroll_x: f32,
    pub background: Color,
    pub focused_background: Color,
    pub border_color: Color,
    pub focused_border_color: Color,
    pub border_width: f32,
    pub radius: BorderRadius,
    pub opacity: f32,
    pub text_color: Color,
    pub selection_background: Color,
    pub cursor_color: Color,
    pub cursor_width: f32,
    pub text_style: TextStyle,
    pub text_options: TextOptions,
    pub padding: LogicalInsets,
    pub read_only: bool,
    pub keyboard_kind: KeyboardKind,
    pub request_caps: bool,
    pub accept_button_text: String,
    pub accept_button_enabled: bool,
    pub delete_button_enabled: bool,
}

impl TextInput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    pub fn focused_background(mut self, color: Color) -> Self {
        self.focused_background = color;
        self
    }

    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border_width = width;
        self.border_color = color;
        self.focused_border_color = color;
        self
    }

    pub fn focused_border(mut self, color: Color) -> Self {
        self.focused_border_color = color;
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = BorderRadius {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        };
        self
    }

    pub fn border_radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = color;
        self
    }

    pub fn text_style(mut self, style: TextStyle) -> Self {
        self.text_style = style;
        self
    }

    pub fn text_options(mut self, options: TextOptions) -> Self {
        self.text_options = options;
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

    pub fn text_weight(mut self, weight: FontWeight) -> Self {
        self.text_style.weight = weight;
        self
    }

    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = LogicalInsets::uniform(padding);
        self
    }

    pub fn insets(mut self, insets: LogicalInsets) -> Self {
        self.padding = insets;
        self
    }

    pub fn render(&mut self, ui: &mut Ui, area: LogicalRect) -> TextInputResponse {
        self.normalize_offsets();
        let inner = area.inset(self.padding);
        let old_focused = self.focused;
        let old_cursor = self.cursor;
        let old_anchor = self.anchor;
        let old_scroll = self.scroll_x;
        let mut response = TextInputResponse::default();

        match ui.input().clone() {
            Input::PointerDown { position } => {
                self.focused = area.contains(position.x, position.y);
                if self.focused {
                    self.cursor = ui.text_offset_at_position(&self.request(inner), position);
                    self.anchor = self.cursor;
                }
            }
            Input::Char(character)
                if self.focused && !self.read_only && !character.is_control() =>
            {
                self.delete_selection();
                self.text.insert(self.cursor, character);
                self.cursor += character.len_utf8();
                self.anchor = self.cursor;
                response.edited = true;
            }
            Input::Backspace if self.focused && !self.read_only => {
                if self.delete_selection() {
                    response.edited = true;
                } else if self.cursor != 0 {
                    let previous = self.text[..self.cursor]
                        .grapheme_indices(true)
                        .next_back()
                        .map_or(0, |(offset, _)| offset);
                    self.text.drain(previous..self.cursor);
                    self.cursor = previous;
                    self.anchor = previous;
                    response.edited = true;
                }
            }
            Input::Delete if self.focused && !self.read_only => {
                if self.delete_selection() {
                    response.edited = true;
                } else if self.cursor < self.text.len() {
                    let next = self.cursor
                        + self.text[self.cursor..]
                            .graphemes(true)
                            .next()
                            .map_or(0, str::len);
                    self.text.drain(self.cursor..next);
                    self.anchor = self.cursor;
                    response.edited = true;
                }
            }
            Input::CursorLeft if self.focused => {
                self.cursor = if self.cursor != self.anchor {
                    self.cursor.min(self.anchor)
                } else {
                    self.text[..self.cursor]
                        .grapheme_indices(true)
                        .next_back()
                        .map_or(0, |(offset, _)| offset)
                };
                self.anchor = self.cursor;
            }
            Input::CursorRight if self.focused => {
                self.cursor = if self.cursor != self.anchor {
                    self.cursor.max(self.anchor)
                } else {
                    self.cursor
                        + self.text[self.cursor..]
                            .graphemes(true)
                            .next()
                            .map_or(0, str::len)
                };
                self.anchor = self.cursor;
            }
            Input::Enter if self.focused => response.accepted = true,
            _ => {}
        }

        let cursor = ui.text_cursor_rect(&self.request(inner), self.cursor);
        if cursor.x < inner.x {
            self.scroll_x = (self.scroll_x - (inner.x - cursor.x)).max(0.0);
        } else if cursor.x + self.cursor_width > inner.x + inner.width {
            self.scroll_x += cursor.x + self.cursor_width - inner.x - inner.width;
        }

        if response.edited
            || self.focused != old_focused
            || self.cursor != old_cursor
            || self.anchor != old_anchor
            || self.scroll_x != old_scroll
        {
            ui.invalidate(area);
        }

        Rectangle::new(area)
            .background(if self.focused {
                self.focused_background
            } else {
                self.background
            })
            .border(
                self.border_width,
                if self.focused {
                    self.focused_border_color
                } else {
                    self.border_color
                },
            )
            .border_radius(self.radius)
            .opacity(self.opacity)
            .render(ui);

        let request = self.request(inner);
        if self.cursor != self.anchor {
            let start = ui.text_cursor_rect(&request, self.cursor.min(self.anchor));
            let end = ui.text_cursor_rect(&request, self.cursor.max(self.anchor));
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

        Text::new(&self.text)
            .in_area(inner)
            .offset_x(self.scroll_x)
            .color(self.text_color)
            .font(self.text_style.font)
            .size(self.text_style.size)
            .weight(self.text_style.weight)
            .options(request.options)
            .render(ui);

        if self.focused {
            let cursor = ui.text_cursor_rect(&request, self.cursor);
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
            ui.show_keyboard(&KeyboardRequest {
                kind: self.keyboard_kind,
                request_caps: self.request_caps,
                accept_button_text: &self.accept_button_text,
                accept_button_enabled: self.accept_button_enabled,
                delete_button_enabled: self.delete_button_enabled && !self.text.is_empty(),
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
            text: &self.text,
            area,
            offset_x: self.scroll_x,
            color: self.text_color,
            style: self.text_style,
            options,
        }
    }

    fn normalize_offsets(&mut self) {
        self.cursor = self.cursor.min(self.text.len());
        while !self.text.is_char_boundary(self.cursor) {
            self.cursor -= 1;
        }
        self.anchor = self.anchor.min(self.text.len());
        while !self.text.is_char_boundary(self.anchor) {
            self.anchor -= 1;
        }
    }

    fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    fn delete_selection(&mut self) -> bool {
        let selection = self.selection();
        if selection.is_empty() {
            return false;
        }
        self.cursor = selection.start;
        self.anchor = selection.start;
        self.text.drain(selection);
        true
    }
}

impl Default for TextInput {
    fn default() -> Self {
        Self {
            text: String::new(),
            focused: false,
            cursor: 0,
            anchor: 0,
            scroll_x: 0.0,
            background: Color::from_rgba8(205, 210, 220, 255),
            focused_background: Color::from_rgba8(245, 245, 250, 255),
            border_color: Color::TRANSPARENT,
            focused_border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: BorderRadius::default(),
            opacity: 1.0,
            text_color: Color::BLACK,
            selection_background: Color::from_rgba8(70, 110, 190, 128),
            cursor_color: Color::BLACK,
            cursor_width: 1.0,
            text_style: TextStyle::default(),
            text_options: TextOptions::default(),
            padding: LogicalInsets::uniform(8.0),
            read_only: false,
            keyboard_kind: KeyboardKind::default(),
            request_caps: false,
            accept_button_text: String::new(),
            accept_button_enabled: true,
            delete_button_enabled: true,
        }
    }
}
