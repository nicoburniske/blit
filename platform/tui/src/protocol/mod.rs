mod parser;

pub use parser::Parser;

use blit_tui_render::color::PaletteSlot;
use std::io::{self, Write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Text(char),
    Key(Key),
    Mouse {
        kind: MouseKind,
        modifiers: Modifiers,
        column: u16,
        row: u16,
    },
    Focus(bool),
    Theme(bool),
    Color {
        slot: PaletteSlot,
        rgb: [u8; 3],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub shifted: Option<char>,
    pub modifiers: Modifiers,
    pub kind: KeyKind,
    pub text: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyCode {
    Character(char),
    Escape,
    Enter,
    Tab,
    Backspace,
    Insert,
    Delete,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Function(u8),
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub super_key: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    Down(MouseButton),
    Up(MouseButton),
    Move,
    Scroll { x: i8, y: i8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
    Other(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    Press,
    Repeat,
    Release,
}

// disambiguate keys and report repeats, releases and alternate keys
// save and enable focus (1004) and theme (2031) notifications on entry, restore on exit
pub const ENTER: &[u8] = b"\x1b[?1049h\x1b[?25l\x1b[?1003h\x1b[?1006h\x1b[?2004h\x1b[>7u\x1b[?1004s\x1b[?2031s\x1b[?1004h\x1b[?2031h";
pub const LEAVE: &[u8] = b"\x1b[?2031l\x1b[?1004l\x1b[?2031r\x1b[?1004r\x1b[<u\x1b[?2004l\x1b[?1006l\x1b[?1003l\x1b[?25h\x1b[?1049l";

pub fn query_colors(output: &mut impl Write) -> io::Result<()> {
    output.write_all(b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\")?;
    for index in 0..=255 {
        write!(output, "\x1b]4;{index};?\x1b\\")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
