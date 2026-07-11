use std::ops::Range;

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
    #[derive(Debug)]
    pub struct TextInput {
        #[skip]
        pub id: WidgetId = WidgetId::unique(),
        pub text: String,
        #[skip]
        pub focused: bool,
        #[skip]
        pub cursor: usize,
        #[skip]
        pub anchor: usize,
        #[skip]
        pub scroll_x: f32,
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
        pub accept_button_text: String,
        pub accept_button_enabled: bool = true,
        pub delete_button_enabled: bool = true,
    }
    features: [padding, border, radius, text_style]
}

impl TextInput {
    pub fn id(mut self, source: impl std::hash::Hash) -> Self {
        self.id = WidgetId::new(source);
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
        let id = ui.id(("text input", self.id));
        let interaction = ui.interact(id, area, Sense::FOCUS);
        self.focused = ui.is_focused(id);

        if interaction.pressed {
            if let Some(position) = ui.pointer_position() {
                self.cursor = ui
                    .platform()
                    .text_offset_at_position(&self.request(inner), position);
                self.anchor = self.cursor;
            }
        }

        match ui.input().clone() {
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

        let cursor = ui
            .platform()
            .text_cursor_rect(&self.request(inner), self.cursor);
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
            .radius(self.radius)
            .opacity(self.opacity)
            .render(ui);

        let request = self.request(inner);
        if self.cursor != self.anchor {
            let start = ui
                .platform()
                .text_cursor_rect(&request, self.cursor.min(self.anchor));
            let end = ui
                .platform()
                .text_cursor_rect(&request, self.cursor.max(self.anchor));
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
            .text_size(self.text_style.size)
            .text_weight(self.text_style.weight)
            .options(request.options)
            .render(ui);

        if self.focused {
            let cursor = ui.platform().text_cursor_rect(&request, self.cursor);
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
            intrinsic_height: false,
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

impl SizedComponent for &mut TextInput {
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
