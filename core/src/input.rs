use crate::geometry::LogicalPoint;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Input {
    #[default]
    None,
    PointerDown {
        position: LogicalPoint,
        button: PointerButton,
        modifiers: Modifiers,
    },
    PointerUp {
        position: LogicalPoint,
        button: PointerButton,
        modifiers: Modifiers,
        leave: bool,
    },
    PointerMove {
        position: LogicalPoint,
        modifiers: Modifiers,
    },
    PointerLeave,
    Scroll {
        position: LogicalPoint,
        delta_x: f32,
        delta_y: f32,
        modifiers: Modifiers,
    },
    /// committed text from a keyboard or ime
    Text(char),
    /// a logical key press
    Key(KeyInput),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyInput {
    pub key: Key,
    pub modifiers: Modifiers,
    pub pressed: bool,
    pub repeat: bool,
}

impl KeyInput {
    pub const fn new(key: Key) -> Self {
        Self { key, modifiers: Modifiers::NONE, pressed: true, repeat: false }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// a logical character key for shortcuts
    Character(char),
    Backspace,
    Delete,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Enter,
    Tab,
    Escape,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    /// function key number starting at one
    Function(u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const ALT: Self = Self(1 << 2);
    pub const CONTROL: Self = Self(1 << 1);
    pub const NONE: Self = Self(0);
    pub const SHIFT: Self = Self(1 << 0);
    pub const SUPER: Self = Self(1 << 3);

    pub const fn new(shift: bool, control: bool, alt: bool, super_key: bool) -> Self {
        Self(
            shift as u8 * Self::SHIFT.0
                | control as u8 * Self::CONTROL.0
                | alt as u8 * Self::ALT.0
                | super_key as u8 * Self::SUPER.0,
        )
    }

    pub const fn contains(self, modifiers: Self) -> bool { self.0 & modifiers.0 == modifiers.0 }

    pub const fn shift(self) -> bool { self.contains(Self::SHIFT) }

    pub const fn control(self) -> bool { self.contains(Self::CONTROL) }

    pub const fn alt(self) -> bool { self.contains(Self::ALT) }

    pub const fn super_key(self) -> bool { self.contains(Self::SUPER) }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PointerButton {
    #[default]
    Primary,
    Secondary,
    Middle,
    Back,
    Forward,
    Other(u16),
}
