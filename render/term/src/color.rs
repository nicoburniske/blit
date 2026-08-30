#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Color {
    #[default]
    Reset,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    pub const BLACK: Self = Self::Indexed(0);
    pub const RED: Self = Self::Indexed(1);
    pub const GREEN: Self = Self::Indexed(2);
    pub const YELLOW: Self = Self::Indexed(3);
    pub const BLUE: Self = Self::Indexed(4);
    pub const MAGENTA: Self = Self::Indexed(5);
    pub const CYAN: Self = Self::Indexed(6);
    pub const GRAY: Self = Self::Indexed(7);
    pub const DARK_GRAY: Self = Self::Indexed(8);
    pub const LIGHT_RED: Self = Self::Indexed(9);
    pub const LIGHT_GREEN: Self = Self::Indexed(10);
    pub const LIGHT_YELLOW: Self = Self::Indexed(11);
    pub const LIGHT_BLUE: Self = Self::Indexed(12);
    pub const LIGHT_MAGENTA: Self = Self::Indexed(13);
    pub const LIGHT_CYAN: Self = Self::Indexed(14);
    pub const WHITE: Self = Self::Indexed(15);
}
