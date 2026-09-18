use blit::{Axis, Sizing};

blit::builder! {
    /// sizing policy for a flow layout child
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Item {
        new(),
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
    }
}

impl Item {
    pub fn fixed(mut self, width: f32, height: f32) -> Self {
        self.width = Sizing::fixed(width);
        self.height = Sizing::fixed(height);
        self
    }

    pub fn grow(mut self) -> Self {
        self.width = Sizing::grow();
        self.height = Sizing::grow();
        self
    }

    pub fn sizing(&self, axis: Axis) -> Sizing {
        match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => self.height,
        }
    }
}

pub fn item() -> Item {
    Item::new()
}
