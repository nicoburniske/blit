use blit::{Atom, Constraints, LogicalRect, Size};

use crate::TuiPlatform;

blit::builder! {
    /// tints previously painted cells without replacing their text
    ///
    /// opacity ranges from zero to 255. Kitty images are unaffected.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Tint {
        new(color: [u8; 3], opacity: u8),
    }
}

impl Atom<TuiPlatform> for Tint {
    fn measure(&self, _: &mut TuiPlatform, _: Constraints) -> Size {
        Size::ZERO
    }

    fn paint(&self, platform: &mut TuiPlatform, area: LogicalRect) {
        platform.cells(area).tint(self.color, self.opacity);
    }

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        area
    }
}
