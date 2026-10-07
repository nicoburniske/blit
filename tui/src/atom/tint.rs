use blit::{Atom, Constraints, PhysicalRect, Size};

use crate::TuiContext;

blit::builder! {
    /// tints previously painted cells without replacing their text
    ///
    /// opacity ranges from zero to 255. Kitty images are unaffected.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Tint {
        new(color: [u8; 3], opacity: u8),
    }
}

impl Atom<TuiContext> for Tint {
    fn measure(&self, _: &mut TuiContext, _: Constraints<i32>) -> Size<i32> {
        Size::ZERO
    }

    fn paint(&self, context: &mut TuiContext, area: PhysicalRect) {
        context.cells(area).tint(self.color, self.opacity);
    }

    fn paint_bounds(&self, area: PhysicalRect) -> PhysicalRect {
        area
    }
}
