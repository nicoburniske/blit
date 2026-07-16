#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    pub const BLACK: Self = Self::from_rgba8(0, 0, 0, 255);
    pub const GRAY: Self = Self::from_rgba8(128, 128, 128, 255);
    pub const TRANSPARENT: Self = Self::from_rgba8(0, 0, 0, 0);
    pub const WHITE: Self = Self::from_rgba8(255, 255, 255, 255);

    pub const fn from_rgba8(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self { red, green, blue, alpha }
    }
}
