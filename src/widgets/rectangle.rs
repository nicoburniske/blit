use crate::{Color, LogicalRect, Ui};

crate::component! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Rectangle<'a> {
        new(pub area: LogicalRect);
        pub background: Color,
        #[skip]
        pub border: Border<'a>,
        pub radius: BorderRadius,
        pub opacity: f32 = 1.0,
    }
    features: [radius]
}

impl<'a> Rectangle<'a> {
    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border = Border::Solid { width, color };
        self
    }

    pub fn gradient_border(mut self, width: f32, gradient: LinearGradient<'a>) -> Self {
        self.border = Border::Gradient { width, gradient };
        self
    }

    pub fn shadow(self, shadow: BoxShadow) -> ShadowedRectangle<'a> {
        ShadowedRectangle {
            rectangle: self,
            shadow,
        }
    }

    pub fn render(self, ui: &mut Ui) {
        if let Some(bounds) = ui.draw_bounds(self.area) {
            ui.platform().draw_rectangle(&self, bounds);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowedRectangle<'a> {
    pub rectangle: Rectangle<'a>,
    pub shadow: BoxShadow,
}

impl ShadowedRectangle<'_> {
    pub fn render(self, ui: &mut Ui) {
        let request = BoxShadowRequest {
            area: self.rectangle.area,
            radius: self.rectangle.radius,
            shadow: self.shadow,
        };
        if let Some(bounds) = ui.draw_bounds(request.bounds()) {
            ui.platform().draw_box_shadow(&request, bounds);
        }
        self.rectangle.render(ui);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Border<'a> {
    #[default]
    None,
    Solid {
        width: f32,
        color: Color,
    },
    Gradient {
        width: f32,
        gradient: LinearGradient<'a>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearGradient<'a> {
    pub stops: &'a [GradientStop],
    pub angle_degrees: f32,
}

impl<'a> LinearGradient<'a> {
    pub const fn new(stops: &'a [GradientStop]) -> Self {
        Self {
            stops,
            angle_degrees: 0.0,
        }
    }

    pub const fn angle(mut self, angle: f32) -> Self {
        self.angle_degrees = angle;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub position: f32,
    pub color: Color,
}

impl GradientStop {
    pub const fn new(position: f32, color: Color) -> Self {
        Self { position, color }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadowRequest {
    pub area: LogicalRect,
    pub radius: BorderRadius,
    pub shadow: BoxShadow,
}

impl BoxShadowRequest {
    pub fn bounds(self) -> LogicalRect {
        let blur = self.shadow.blur.max(0.0);
        let outset = self.shadow.spread + blur;
        LogicalRect {
            x: self.area.x + self.shadow.offset_x - outset,
            y: self.area.y + self.shadow.offset_y - outset,
            width: self.area.width + outset * 2.0,
            height: self.area.height + outset * 2.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

crate::component! {
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct BoxShadow {
        new(pub color: Color);
        #[skip]
        pub offset_x: f32,
        #[skip]
        pub offset_y: f32,
        pub blur: f32,
        pub spread: f32,
    }
    features: []
}

impl BoxShadow {
    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }
}
