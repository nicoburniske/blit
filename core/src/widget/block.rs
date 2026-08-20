use super::Widget;
use crate::{
    Clip, Element, Layout, Sizing, Ui,
    color::Color,
    interact::{Interaction, Sense, WidgetId},
    paint::BorderRadius,
};

pub struct Block<'a> {
    element: Element<'a>,
}

impl Block<'_> {
    pub const fn new() -> Self {
        Self {
            element: Element::new(Layout::vertical()),
        }
    }

    pub const fn width(mut self, width: Sizing) -> Self {
        self.element.layout.width = width;
        self
    }

    pub const fn height(mut self, height: Sizing) -> Self {
        self.element.layout.height = height;
        self
    }

    pub const fn fixed(mut self, width: f32, height: f32) -> Self {
        self.element.layout.width = Sizing::fixed(width);
        self.element.layout.height = Sizing::fixed(height);
        self
    }

    pub const fn background(mut self, color: Color) -> Self {
        self.element.appearance.background = color;
        self
    }

    pub const fn border(mut self, width: f32, color: Color) -> Self {
        self.element = self.element.border(width, color);
        self
    }

    pub const fn radius(mut self, radius: BorderRadius) -> Self {
        self.element.appearance.radius = radius;
        self
    }

    pub const fn uniform_radius(mut self, radius: f32) -> Self {
        self.element = self.element.uniform_radius(radius);
        self
    }

    pub const fn opacity(mut self, opacity: f32) -> Self {
        self.element.appearance.opacity = opacity;
        self
    }

    pub const fn clip(mut self, clip: Clip) -> Self {
        self.element.clip = clip;
        self
    }

    pub const fn id(mut self, id: WidgetId) -> Self {
        self.element.id = Some(id);
        self
    }

    pub const fn interact(mut self, id: WidgetId, sense: Sense) -> Self {
        self.element.interaction = Some((id, sense));
        self
    }
}

impl Default for Block<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Block<'_> {
    type Output = Interaction;

    fn build(self, ui: &mut Ui) -> Interaction {
        ui.leaf(self.element)
    }
}
