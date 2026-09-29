//! logical and terminal cell coordinates
//!
//! painting selects cells whose centers lie inside a half open logical rectangle.
//! mouse input sends those same centers to kernel hit testing.

use blit::{LogicalPoint, LogicalRect, PhysicalRect};

/// selects cell coordinates whose centers lie inside `area`
#[inline]
pub fn cell_rect(area: LogicalRect) -> PhysicalRect {
    let x = (area.x - 0.5).ceil() as i32;
    let y = (area.y - 0.5).ceil() as i32;
    let right = (area.x + area.width - 0.5).ceil() as i32;
    let bottom = (area.y + area.height - 0.5).ceil() as i32;
    PhysicalRect::new(x, y, right.saturating_sub(x).max(0), bottom.saturating_sub(y).max(0))
}

/// maps a terminal cell to its logical center for hit testing
#[inline]
pub fn cell_center(column: u16, row: u16) -> LogicalPoint {
    LogicalPoint::new(f32::from(column) + 0.5, f32::from(row) + 0.5)
}
