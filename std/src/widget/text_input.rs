use blit::{Input, Key};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct State {
    pub cursor: usize,
    pub anchor: usize,
    pub offset_x: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Response {
    pub changed: bool,
    pub submitted: bool,
}

impl State {
    pub fn update(&mut self, value: &mut String, input: &Input) -> Response {
        self.cursor = boundary(value, self.cursor);
        self.anchor = boundary(value, self.anchor);
        self.offset_x = self.offset_x.max(0.0);
        let mut response = Response::default();
        match *input {
            Input::Text(character) if !character.is_control() => {
                self.delete_selection(value);
                let end = self.cursor + character.len_utf8();
                value.insert(self.cursor, character);
                self.cursor = ceil_boundary(value, end);
                self.anchor = self.cursor;
                response.changed = true;
            }
            Input::Key(key) if key.pressed => match key.key {
                Key::Character('a' | 'A')
                    if key.modifiers.control() || key.modifiers.super_key() =>
                {
                    self.anchor = 0;
                    self.cursor = value.len();
                }
                Key::Backspace => {
                    response.changed = self.delete_selection(value);
                    if !response.changed && self.cursor != 0 {
                        let start = previous_boundary(value, self.cursor);
                        value.replace_range(start..self.cursor, "");
                        self.cursor = start;
                        self.anchor = start;
                        response.changed = true;
                    }
                }
                Key::Delete => {
                    response.changed = self.delete_selection(value);
                    if !response.changed && self.cursor != value.len() {
                        let end = next_boundary(value, self.cursor);
                        value.replace_range(self.cursor..end, "");
                        self.anchor = self.cursor;
                        response.changed = true;
                    }
                }
                Key::ArrowLeft => {
                    let (start, end) = self.selection();
                    let cursor = if !key.modifiers.shift() && start != end {
                        start
                    } else {
                        previous_boundary(value, self.cursor)
                    };
                    self.move_to(value, cursor, key.modifiers.shift());
                }
                Key::ArrowRight => {
                    let (start, end) = self.selection();
                    let cursor = if !key.modifiers.shift() && start != end {
                        end
                    } else {
                        next_boundary(value, self.cursor)
                    };
                    self.move_to(value, cursor, key.modifiers.shift());
                }
                Key::Home => self.move_to(value, 0, key.modifiers.shift()),
                Key::End => self.move_to(value, value.len(), key.modifiers.shift()),
                Key::Enter if !key.repeat => response.submitted = true,
                _ => {}
            },
            _ => {}
        }
        response
    }

    pub fn move_to(&mut self, value: &str, offset: usize, extend: bool) {
        self.cursor = boundary(value, offset);
        if !extend {
            self.anchor = self.cursor;
        }
    }

    fn selection(&self) -> (usize, usize) {
        (self.cursor.min(self.anchor), self.cursor.max(self.anchor))
    }

    fn delete_selection(&mut self, value: &mut String) -> bool {
        let (start, end) = self.selection();
        if start == end {
            return false;
        }
        value.replace_range(start..end, "");
        self.cursor = start;
        self.anchor = start;
        true
    }
}

fn previous_boundary(value: &str, offset: usize) -> usize {
    value[..offset]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(start, _)| start)
}

fn next_boundary(value: &str, offset: usize) -> usize {
    offset + value[offset..].graphemes(true).next().map_or(0, str::len)
}

fn boundary(value: &str, offset: usize) -> usize {
    if offset >= value.len() {
        return value.len();
    }
    value
        .grapheme_indices(true)
        .map(|(offset, _)| offset)
        .take_while(|boundary| *boundary <= offset)
        .last()
        .unwrap_or(0)
}

fn ceil_boundary(value: &str, offset: usize) -> usize {
    value
        .grapheme_indices(true)
        .map(|(offset, _)| offset)
        .find(|boundary| *boundary >= offset)
        .unwrap_or(value.len())
}
