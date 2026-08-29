use super::Widget;
use crate::{
    Ui, animation::Transition, container::Slot, interact::WidgetId, node::Content, style::Style,
};

crate::builder! {
    /// styled rectangular leaf
    ///
    /// transitions require `id`
    pub struct Rectangle<'a> {
        new(),
        @optional {
            id: WidgetId,
            transition: Transition,
        },
        slot: Slot = Slot::new(),
        style: Style<'a> = Style::new(),
    }
}

impl Default for Rectangle<'static> {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for Rectangle<'_> {
    type Output = ();

    fn render(self, ui: &mut Ui) {
        let node = ui.add_leaf(self.slot, Content::Rectangle(self.style));
        if let Some(id) = self.id {
            ui.set_node_id(node, id);
            if let Some(transition) = self.transition {
                ui.set_node_transition(node, id, transition);
            }
        }
    }
}
