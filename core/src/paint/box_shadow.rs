use super::BorderRadius;
use crate::{color::Color, geometry::LogicalRect, Ui};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    pub area: LogicalRect,
    pub color: Color,
    pub radius: BorderRadius,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
}

impl BoxShadow {
    pub fn new(area: LogicalRect, color: Color) -> Self {
        Self {
            area,
            color,
            radius: BorderRadius::default(),
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
        }
    }

    pub fn radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }

    pub fn blur(mut self, blur: f32) -> Self {
        self.blur = blur;
        self
    }

    pub fn spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    pub fn uniform_radius(mut self, radius: f32) -> Self {
        self.radius =
            BorderRadius { top_left: radius, top_right: radius, bottom_right: radius, bottom_left: radius };
        self
    }

    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }

    pub fn bounds(self) -> LogicalRect {
        let blur = self.blur.max(0.0);
        let outset = self.spread + blur;
        LogicalRect {
            x: self.area.x + self.offset_x - outset,
            y: self.area.y + self.offset_y - outset,
            width: self.area.width + outset * 2.0,
            height: self.area.height + outset * 2.0,
        }
    }

    pub fn render(self, ui: &mut Ui) { ui.paint_box_shadow(self); }
}
