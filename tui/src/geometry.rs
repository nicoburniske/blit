use blit::{LogicalPoint, LogicalRect, PhysicalRect};

/// selects cells whose centers lie inside the rectangle
#[inline]
pub fn cell_rect(area: LogicalRect) -> PhysicalRect {
    let x = (area.x - 0.5).ceil() as i32;
    let y = (area.y - 0.5).ceil() as i32;
    let right = (area.x + area.width - 0.5).ceil() as i32;
    let bottom = (area.y + area.height - 0.5).ceil() as i32;
    PhysicalRect {
        x,
        y,
        width: right.saturating_sub(x).max(0),
        height: bottom.saturating_sub(y).max(0),
    }
}

/// maps terminal input to the same cell centers used for painting
#[inline]
pub fn cell_center(column: u16, row: u16) -> LogicalPoint {
    LogicalPoint::new(f32::from(column) + 0.5, f32::from(row) + 0.5)
}
