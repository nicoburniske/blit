use blit::{Atom, AtomCx, Constraints, PhysicalRect, Size};

use crate::Context;

blit::builder! {
    /// tints previously painted cells without replacing their text
    ///
    /// opacity ranges from zero to 255. Kitty images are unaffected.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Tint {
        new(color: [u8; 3], opacity: u8),
    }
}

impl<C: Context> Atom<C> for Tint {
    fn measure(&self, _: &mut AtomCx<'_, C>, _: Constraints<i32>) -> Size<i32> {
        Size::ZERO
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: PhysicalRect) {
        cx.context_mut().as_mut().cells(area).tint(self.color, self.opacity);
    }
}
