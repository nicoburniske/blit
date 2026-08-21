use super::Widget;
use crate::{
    Ui,
    animation::Transition,
    color::Color,
    container::{Item, Sizing},
    interact::WidgetId,
    node::Content,
    style::{Border, BorderRadius, Style},
};

pub struct Rectangle {
    id: Option<WidgetId>,
    item: Item,
    style: Style<'static>,
    transition: Option<Transition>,
}

impl Rectangle {
    pub fn new() -> Self {
        Self {
            id: None,
            item: Item::new(),
            style: Style::new(),
            transition: None,
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
        self.style.background = color;
        self
    }

    pub const fn border(mut self, width: f32, color: Color) -> Self {
        self.style.border = Border::Solid { width, color };
        self
    }

    pub const fn radius(mut self, radius: BorderRadius) -> Self {
        self.style.radius = radius;
        self
    }

    pub const fn uniform_radius(mut self, radius: f32) -> Self {
        self.style.radius = BorderRadius {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        };
        self
    }

    pub const fn opacity(mut self, opacity: f32) -> Self {
        self.style.opacity = opacity;
        self
    }

    pub const fn id(mut self, id: WidgetId) -> Self {
        self.id = Some(id);
        self
    }

    pub const fn transition(mut self, transition: Transition) -> Self {
        self.transition = Some(transition);
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

    fn render(self, ui: &mut Ui) {
        let node = ui.add_leaf(self.item, Content::Rectangle(self.style));
        if let Some(id) = self.id {
            ui.set_node_id(node, id);
            if let Some(transition) = self.transition {
                ui.set_node_transition(node, id, transition);
            }
        }
    }
}
