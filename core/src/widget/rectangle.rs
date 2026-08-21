use super::Widget;
use crate::{
    Content, Item, Sizing, Style, Ui,
    color::Color,
    interact::WidgetId,
    paint::{Border, BorderRadius},
};

pub struct Rectangle {
    id: Option<WidgetId>,
    item: Item,
    appearance: Style<'static>,
}

impl Rectangle {
    pub fn new() -> Self {
        Self {
            id: None,
            item: Item::new(),
            appearance: Style::new(),
        }
    }

    pub const fn width(mut self, width: Sizing) -> Self {
        self.item.width = width;
        self
    }

    pub const fn height(mut self, height: Sizing) -> Self {
        self.item.height = height;
        self
    }

    pub const fn fixed(mut self, width: f32, height: f32) -> Self {
        self.item.width = Sizing::fixed(width);
        self.item.height = Sizing::fixed(height);
        self
    }

    pub const fn background(mut self, color: Color) -> Self {
        self.appearance.background = color;
        self
    }

    pub const fn border(mut self, width: f32, color: Color) -> Self {
        self.appearance.border = Border::Solid { width, color };
        self
    }

    pub const fn radius(mut self, radius: BorderRadius) -> Self {
        self.appearance.radius = radius;
        self
    }

    pub const fn uniform_radius(mut self, radius: f32) -> Self {
        self.appearance.radius = BorderRadius {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        };
        self
    }

    pub const fn opacity(mut self, opacity: f32) -> Self {
        self.appearance.opacity = opacity;
        self
    }

    pub const fn id(mut self, id: WidgetId) -> Self {
        self.id = Some(id);
        self
    }
}

impl Default for Rectangle {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Rectangle {
    type Output = ();

    fn build(self, ui: &mut Ui) {
        let node = ui.add_leaf(self.item, Content::Rectangle(self.appearance));
        if let Some(id) = self.id {
            ui.set_node_id(node, id);
        }
    }
}
