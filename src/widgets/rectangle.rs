use crate::{Color, LogicalRect, Ui};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

crate::component! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Rectangle {
        new(pub area: LogicalRect);
        pub background: Color,
        pub border_color: Color,
        pub border_width: f32,
        pub radius: BorderRadius,
        pub opacity: f32 = 1.0,
    }
    features: [border, radius]
}

impl Rectangle {
    pub fn render(self, ui: &mut Ui) {
        if let Some(bounds) = ui.draw_bounds(self.area) {
            ui.platform().draw_rectangle(&self, bounds);
        }
    }
}
