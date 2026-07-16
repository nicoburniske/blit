use crate::geometry::LogicalPoint;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Input {
    #[default]
    None,
    PointerDown {
        position: LogicalPoint,
    },
    PointerUp {
        position: LogicalPoint,
        leave: bool,
    },
    PointerMove {
        position: LogicalPoint,
    },
    PointerLeave,
    Scroll {
        position: LogicalPoint,
        delta_x: f32,
        delta_y: f32,
    },
    Char(char),
    Backspace,
    Delete,
    CursorLeft,
    CursorRight,
    Enter,
    Tab,
}
