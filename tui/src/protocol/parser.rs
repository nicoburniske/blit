use super::{Event, Key, KeyCode, KeyKind, Modifiers, MouseButton, MouseKind, PaletteSlot};

#[derive(Default)]
pub struct Parser {
    state: State,
    sequence: Vec<u8>,
    utf8: [u8; 4],
    utf8_len: usize,
}

impl Parser {
    pub fn parse(&mut self, bytes: &[u8], mut emit: impl FnMut(Event)) {
        for &byte in bytes {
            match self.state {
                State::Paste(matched) => {
                    const END: &[u8] = b"\x1b[201~";
                    if byte == END[matched] {
                        self.state = if matched + 1 == END.len() {
                            self.utf8_len = 0;
                            State::Ground
                        } else {
                            State::Paste(matched + 1)
                        };
                    } else {
                        for &literal in &END[..matched] {
                            self.text(literal, &mut emit);
                        }
                        let matched = usize::from(byte == END[0]);
                        self.state = State::Paste(matched);
                        if matched == 0 {
                            self.text(byte, &mut emit);
                        }
                    }
                }
                State::Ground => match byte {
                    0x1b => {
                        self.utf8_len = 0;
                        self.state = State::Escape;
                    }
                    b'\r' | b'\t' | 0x7f => {
                        self.utf8_len = 0;
                        emit(Event::Key(Key {
                            code: codepoint(u32::from(byte)).unwrap(),
                            shifted: None,
                            modifiers: Modifiers::default(),
                            kind: KeyKind::Press,
                            text: false,
                        }));
                    }
                    0x00..=0x1f => self.utf8_len = 0,
                    _ => self.text(byte, &mut emit),
                },
                State::Escape => {
                    self.sequence.clear();
                    self.state = match byte {
                        b'[' => State::Csi,
                        b']' => State::Osc,
                        b'P' | b'_' | b'^' | b'X' => State::Discard,
                        0x1b => State::Escape,
                        0x20..=0x2f => State::Escape,
                        _ => State::Ground,
                    };
                }
                State::Csi => match byte {
                    0x40..=0x7e => {
                        self.state = State::Ground;
                        if byte == b'~' && self.sequence == b"200" {
                            self.state = State::Paste(0);
                        } else {
                            csi(&self.sequence, byte, &mut emit);
                        }
                    }
                    0x1b => self.state = State::Escape,
                    0x18 | 0x1a => self.state = State::Ground,
                    0x20..=0x3f if self.sequence.len() < MAX_SEQUENCE => self.sequence.push(byte),
                    _ => self.state = State::DiscardCsi,
                },
                State::DiscardCsi => match byte {
                    0x40..=0x7e | 0x18 | 0x1a => self.state = State::Ground,
                    0x1b => self.state = State::Escape,
                    _ => {}
                },
                State::Osc => match byte {
                    0x07 => {
                        osc(&self.sequence, &mut emit);
                        self.state = State::Ground;
                    }
                    0x1b => self.state = State::StringEnd(true),
                    0x18 | 0x1a => self.state = State::Ground,
                    _ if self.sequence.len() < MAX_SEQUENCE => self.sequence.push(byte),
                    _ => self.state = State::Discard,
                },
                State::Discard => match byte {
                    0x07 | 0x18 | 0x1a => self.state = State::Ground,
                    0x1b => self.state = State::StringEnd(false),
                    _ => {}
                },
                State::StringEnd(osc_reply) => {
                    self.state = match byte {
                        b'\\' => {
                            if osc_reply {
                                osc(&self.sequence, &mut emit);
                            }
                            State::Ground
                        }
                        b'[' => {
                            self.sequence.clear();
                            State::Csi
                        }
                        b']' => {
                            self.sequence.clear();
                            State::Osc
                        }
                        0x1b => State::StringEnd(false),
                        _ => State::Discard,
                    };
                }
            }
        }
        fn csi(bytes: &[u8], final_byte: u8, emit: &mut impl FnMut(Event)) {
            let Ok(parameters) = std::str::from_utf8(bytes) else {
                return;
            };
            match (parameters, final_byte) {
                ("", b'I' | b'O') => emit(Event::Focus(final_byte == b'I')),
                ("?997;1", b'n') => emit(Event::Theme(true)),
                ("?997;2", b'n') => emit(Event::Theme(false)),
                (_, b'M' | b'm') if parameters.starts_with('<') => {
                    let mut fields = parameters[1..].split(';');
                    let parsed = (|| {
                        Some((
                            fields.next()?.parse::<u16>().ok()?,
                            fields.next()?.parse::<u16>().ok()?.checked_sub(1)?,
                            fields.next()?.parse::<u16>().ok()?.checked_sub(1)?,
                        ))
                    })();
                    if let Some((button, column, row)) = parsed
                        && fields.next().is_none()
                    {
                        let modifiers = Modifiers {
                            shift: button & 4 != 0,
                            control: button & 16 != 0,
                            alt: button & 8 != 0,
                            super_key: false,
                        };
                        let pointer_button = match button & 131 {
                            0 => MouseButton::Left,
                            1 => MouseButton::Middle,
                            2 => MouseButton::Right,
                            128 => MouseButton::Back,
                            129 => MouseButton::Forward,
                            130..=131 => MouseButton::Other((button & 3) + 8),
                            _ => MouseButton::Left,
                        };
                        let kind = if button & 64 != 0 {
                            match button & 3 {
                                0 => MouseKind::Scroll { x: 0, y: -1 },
                                1 => MouseKind::Scroll { x: 0, y: 1 },
                                2 => MouseKind::Scroll { x: -1, y: 0 },
                                _ => MouseKind::Scroll { x: 1, y: 0 },
                            }
                        } else if final_byte == b'm' {
                            MouseKind::Up(pointer_button)
                        } else if button & 32 != 0 {
                            MouseKind::Move
                        } else {
                            MouseKind::Down(pointer_button)
                        };
                        emit(Event::Mouse {
                            kind,
                            modifiers,
                            column,
                            row,
                        });
                    }
                }
                (_, b'u' | b'A'..=b'D' | b'H' | b'F' | b'P' | b'Q' | b'S' | b'Z' | b'~') => {
                    if let Some((key, text)) = key(parameters, final_byte) {
                        emit(Event::Key(key));
                        if key.kind != KeyKind::Release {
                            for code in text.into_iter().flat_map(|text| text.split(':')) {
                                if let Some(character) = number(code).and_then(char::from_u32) {
                                    emit(Event::Text(character));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            fn key(parameters: &str, final_byte: u8) -> Option<(Key, Option<&str>)> {
                let mut fields = parameters.split(';');
                let mut codes = fields.next()?.split(':');
                let primary = codes.next()?;
                let code = match final_byte {
                    b'u' => codepoint(number(primary)?)?,
                    b'~' => match number(primary)? {
                        2 => KeyCode::Insert,
                        3 => KeyCode::Delete,
                        5 => KeyCode::PageUp,
                        6 => KeyCode::PageDown,
                        7 => KeyCode::Home,
                        8 => KeyCode::End,
                        n @ 11..=14 => KeyCode::Function((n - 10) as u8),
                        15 => KeyCode::Function(5),
                        n @ 17..=21 => KeyCode::Function((n - 11) as u8),
                        n @ 23..=24 => KeyCode::Function((n - 12) as u8),
                        _ => return None,
                    },
                    _ => {
                        if !primary.is_empty() && primary != "1" {
                            return None;
                        }
                        match final_byte {
                            b'A' => KeyCode::Up,
                            b'B' => KeyCode::Down,
                            b'C' => KeyCode::Right,
                            b'D' => KeyCode::Left,
                            b'H' => KeyCode::Home,
                            b'F' => KeyCode::End,
                            b'P' | b'Q' | b'S' => KeyCode::Function(final_byte - b'P' + 1),
                            b'Z' => KeyCode::Tab,
                            _ => return None,
                        }
                    }
                };
                let shifted = match codes.next().filter(|s| !s.is_empty()) {
                    Some(value) => Some(char::from_u32(number(value)?)?),
                    None => None,
                };
                if let Some(base) = codes.next().filter(|s| !s.is_empty()) {
                    char::from_u32(number(base)?)?;
                }
                if codes.next().is_some() {
                    return None;
                }
                let mut modifiers = fields.next().unwrap_or("").split(':');
                let mask = modifiers.next().unwrap_or("");
                let mask = if mask.is_empty() { 1 } else { number(mask)? };
                let mut mask = u8::try_from(mask.checked_sub(1)?).ok()?;
                if final_byte == b'Z' {
                    mask |= 1;
                }
                let kind = match modifiers.next().unwrap_or("") {
                    "" | "1" => KeyKind::Press,
                    "2" => KeyKind::Repeat,
                    "3" => KeyKind::Release,
                    _ => return None,
                };
                let text = fields.next().filter(|s| !s.is_empty());
                if modifiers.next().is_some()
                    || fields.next().is_some()
                    || text.is_some_and(|text| {
                        text.split(':')
                            .any(|s| number(s).and_then(char::from_u32).is_none())
                    })
                {
                    return None;
                }
                Some((
                    Key {
                        code,
                        shifted,
                        modifiers: Modifiers {
                            shift: mask & 1 != 0,
                            control: mask & 4 != 0,
                            alt: mask & 2 != 0,
                            super_key: mask & 8 != 0,
                        },
                        kind,
                        text: text.is_some(),
                    },
                    text,
                ))
            }
        }
    }

    fn text(&mut self, byte: u8, emit: &mut impl FnMut(Event)) {
        if byte & 0xc0 != 0x80 {
            self.utf8_len = 0;
        }
        self.utf8[self.utf8_len] = byte;
        self.utf8_len += 1;
        match std::str::from_utf8(&self.utf8[..self.utf8_len]) {
            Ok(text) => {
                emit(Event::Text(text.chars().next().unwrap()));
                self.utf8_len = 0;
            }
            Err(error) if error.error_len().is_some() => self.utf8_len = 0,
            Err(_) => {}
        }
    }
}

fn osc(bytes: &[u8], emit: &mut impl FnMut(Event)) {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return;
    };
    let mut fields = text.split(';');
    match fields.next() {
        Some("4") => {
            while let (Some(index), Some(value)) = (fields.next(), fields.next()) {
                if let (Some(index), Some(rgb)) =
                    (number(index).and_then(|n| u8::try_from(n).ok()), rgb(value))
                {
                    emit(Event::Color {
                        slot: PaletteSlot::Indexed(index),
                        rgb,
                    });
                }
            }
        }
        Some(slot @ ("10" | "11")) => {
            let mut slot = if slot == "10" {
                PaletteSlot::Foreground
            } else {
                PaletteSlot::Background
            };
            for value in fields {
                if let Some(rgb) = rgb(value) {
                    emit(Event::Color { slot, rgb });
                }
                if slot == PaletteSlot::Background {
                    break;
                }
                slot = PaletteSlot::Background;
            }
        }
        _ => {}
    }
}

fn rgb(text: &str) -> Option<[u8; 3]> {
    let mut fields = text.strip_prefix("rgb:")?.split('/');
    let mut rgb = [0; 3];
    for channel in &mut rgb {
        let value = fields.next()?;
        if !(1..=4).contains(&value.len()) || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        *channel =
            (u32::from_str_radix(value, 16).ok()? * 255 / ((1 << (4 * value.len())) - 1)) as u8;
    }
    fields.next().is_none().then_some(rgb)
}

fn codepoint(code: u32) -> Option<KeyCode> {
    Some(match code {
        27 | 57344 => KeyCode::Escape,
        13 | 57345 | 57414 => KeyCode::Enter,
        9 | 57346 => KeyCode::Tab,
        127 | 57347 => KeyCode::Backspace,
        57348 | 57425 => KeyCode::Insert,
        57349 | 57426 => KeyCode::Delete,
        57350 | 57417 => KeyCode::Left,
        57351 | 57418 => KeyCode::Right,
        57352 | 57419 => KeyCode::Up,
        57353 | 57420 => KeyCode::Down,
        57354 | 57421 => KeyCode::PageUp,
        57355 | 57422 => KeyCode::PageDown,
        57356 | 57423 => KeyCode::Home,
        57357 | 57424 => KeyCode::End,
        57364..=57398 => KeyCode::Function((code - 57364 + 1) as u8),
        57399..=57408 => KeyCode::Character((b'0' + (code - 57399) as u8) as char),
        57409..=57413 => KeyCode::Character(['.', '/', '*', '-', '+'][(code - 57409) as usize]),
        57415 => KeyCode::Character('='),
        57416 => KeyCode::Character(','),
        0xe000..=0xf8ff => KeyCode::Unknown,
        code => KeyCode::Character(char::from_u32(code)?),
    })
}

fn number(text: &str) -> Option<u32> {
    if text.is_empty() {
        return None;
    }
    text.bytes().try_fold(0u32, |n, b| {
        b.is_ascii_digit().then_some(())?;
        n.checked_mul(10)?.checked_add(u32::from(b - b'0'))
    })
}

#[derive(Default)]
enum State {
    #[default]
    Ground,
    Paste(usize),
    Escape,
    Csi,
    DiscardCsi,
    Osc,
    Discard,
    StringEnd(bool),
}

const MAX_SEQUENCE: usize = 4096;
