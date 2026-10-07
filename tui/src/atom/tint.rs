use blit::{Atom, AtomCx, Constraints, PhysicalRect, Size};

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
    fn measure(&self, _: &mut AtomCx<'_, TuiContext>, _: Constraints<i32>) -> Size<i32> {
        Size::ZERO
    }

    fn paint(&self, cx: &mut AtomCx<'_, TuiContext>, area: PhysicalRect) {
        cx.context_mut().cells(area).tint(self.color, self.opacity);
    }

    fn paint_bounds(&self, area: PhysicalRect) -> PhysicalRect {
        area
    }
}
