use blit::{Key, KeyboardEvent, Scalar};
use unicode_segmentation::UnicodeSegmentation;

/// an editing action independent of its keyboard binding
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Insert(char),
    DeleteBackward,
    DeleteForward,
    MoveLeft {
        extend: bool,
    },
    MoveRight {
        extend: bool,
    },
    MoveToStart {
        extend: bool,
    },
    MoveToEnd {
        extend: bool,
    },
    SelectAll,
    Submit,
    /// releases the widget's keyboard focus
    Blur,
    /// consumes the event without editing
    Ignore,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct State<T> {
    pub cursor: usize,
    pub anchor: usize,
    pub offset_x: T,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Response {
    pub changed: bool,
    pub submitted: bool,
}

impl<T: Scalar> State<T> {
    pub fn update(&mut self, value: &mut String, edit: Option<Edit>) -> Response {
        self.cursor = boundary(value, self.cursor);
        self.anchor = boundary(value, self.anchor);
        self.offset_x = self.offset_x.max(T::ZERO);
        let mut response = Response::default();
        match edit {
            Some(Edit::Insert(character)) if !character.is_control() => {
                self.delete_selection(value);
                let end = self.cursor + character.len_utf8();
                value.insert(self.cursor, character);
                self.cursor = value
                    .grapheme_indices(true)
                    .map(|(offset, _)| offset)
                    .find(|boundary| *boundary >= end)
                    .unwrap_or(value.len());
                self.anchor = self.cursor;
                response.changed = true;
            }
            Some(Edit::SelectAll) => {
                self.anchor = 0;
                self.cursor = value.len();
            }
            Some(Edit::DeleteBackward) => {
                response.changed = self.delete_selection(value);
                if !response.changed && self.cursor != 0 {
                    let start = previous_boundary(value, self.cursor);
                    value.replace_range(start..self.cursor, "");
                    self.cursor = start;
                    self.anchor = start;
                    response.changed = true;
                }
            }
            Some(Edit::DeleteForward) => {
                response.changed = self.delete_selection(value);
                if !response.changed && self.cursor != value.len() {
                    let end = next_boundary(value, self.cursor);
                    value.replace_range(self.cursor..end, "");
                    self.anchor = self.cursor;
                    response.changed = true;
                }
            }
            Some(Edit::MoveLeft { extend }) => {
                let (start, end) = self.selection();
                let cursor = if !extend && start != end {
                    start
                } else {
                    previous_boundary(value, self.cursor)
                };
                self.move_to(value, cursor, extend);
            }
            Some(Edit::MoveRight { extend }) => {
                let (start, end) = self.selection();
                let cursor = if !extend && start != end {
                    end
                } else {
                    next_boundary(value, self.cursor)
                };
                self.move_to(value, cursor, extend);
            }
            Some(Edit::MoveToStart { extend }) => self.move_to(value, 0, extend),
            Some(Edit::MoveToEnd { extend }) => self.move_to(value, value.len(), extend),
            Some(Edit::Submit) => response.submitted = true,
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
}

/// default text editing bindings
///
/// macos uses command and arrow keys or control a and e for start and end.
/// other platforms use home and end. shift extends the selection.
/// up and down move to the start and end on all platforms.
///
/// custom keymaps can fall back to this function for events they do not override.
/// returning `None` lets an event bubble and [`Edit::Ignore`] consumes it.
pub fn default_keymap(event: KeyboardEvent) -> Option<Edit> {
    match event {
        KeyboardEvent::Text(character) if !character.is_control() => Some(Edit::Insert(character)),
        KeyboardEvent::Key(key) if key.pressed => {
            let extend = key.modifiers.shift();
            let macos = cfg!(target_os = "macos");
            let shortcut = if macos {
                key.modifiers.super_key()
            } else {
                key.modifiers.control()
            };
            Some(match key.key {
                Key::Character('a' | 'A') if shortcut => Edit::SelectAll,
                Key::ArrowLeft if macos && shortcut => Edit::MoveToStart { extend },
                Key::ArrowRight if macos && shortcut => Edit::MoveToEnd { extend },
                Key::Character('a' | 'A') if macos && key.modifiers.control() => Edit::MoveToStart { extend },
                Key::Character('e' | 'E') if macos && key.modifiers.control() => Edit::MoveToEnd { extend },
                Key::Backspace => Edit::DeleteBackward,
                Key::Delete => Edit::DeleteForward,
                Key::ArrowLeft => Edit::MoveLeft { extend },
                Key::ArrowRight => Edit::MoveRight { extend },
                Key::ArrowUp => Edit::MoveToStart { extend },
                Key::ArrowDown => Edit::MoveToEnd { extend },
                Key::Home if !macos => Edit::MoveToStart { extend },
                Key::End if !macos => Edit::MoveToEnd { extend },
                Key::Enter if !key.repeat => Edit::Submit,
                Key::Enter => Edit::Ignore,
                Key::Escape => Edit::Blur,
                _ => return None,
            })
        }
        _ => None,
    }
}

impl<T: Scalar> State<T> {
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
