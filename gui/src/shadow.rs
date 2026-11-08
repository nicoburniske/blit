use std::hash::{Hash, Hasher};

use blit::LogicalRect;

use crate::{color::Color, style::BorderRadius};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct BoxShadow {
        new(area: LogicalRect, color: Color),
        radius: BorderRadius = BorderRadius::new(),
        offset_x: f32 = 0.0,
        offset_y: f32 = 0.0,
        blur: f32 = 0.0,
        spread: f32 = 0.0,
        inset: bool = false,
    }
}

impl BoxShadow {
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }

    pub fn bounds(self) -> LogicalRect {
        if self.inset {
            return self.area;
        }
        let outset = self.spread + self.blur.max(0.0);
        LogicalRect {
            x: self.area.x + self.offset_x - outset,
            y: self.area.y + self.offset_y - outset,
            width: self.area.width + outset * 2.0,
            height: self.area.height + outset * 2.0,
        }
    }
}

impl Hash for BoxShadow {
    fn hash<H: Hasher>(&self, state: &mut H) {
        [
            self.area.x,
            self.area.y,
            self.area.width,
            self.area.height,
            self.radius.top_left,
            self.radius.top_right,
            self.radius.bottom_right,
            self.radius.bottom_left,
            self.offset_x,
            self.offset_y,
            self.blur,
            self.spread,
        ]
        .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
        .hash(state);
        self.color.hash(state);
        self.inset.hash(state);
    }
}

#[cfg(test)]
mod tests {
    use std::hash::DefaultHasher;

    use super::*;

    #[test]
    fn signed_zero_preserves_command_hash_equality() {
        let first = BoxShadow::new(LogicalRect::default(), Color::BLACK);
        let second = first.offset(-0.0, -0.0);
        assert_eq!(first, second);
        let mut left = DefaultHasher::new();
        let mut right = DefaultHasher::new();
        first.hash(&mut left);
        second.hash(&mut right);
        assert_eq!(left.finish(), right.finish());
    }
}
