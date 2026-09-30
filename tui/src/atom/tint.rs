use blit::{Atom, LogicalRect};

use crate::TuiContext;

blit::builder! {
    #[const]
    /// tints previously painted cells without replacing their text
    ///
    /// opacity ranges from zero to 255. Kitty images are unaffected.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Tint {
        new(color: [u8; 3], opacity: u8),
    }
}

impl Atom<TuiContext> for Tint {
    fn paint(&self, context: &mut TuiContext, area: LogicalRect) {
        context.cells(area).tint(self.color, self.opacity);
    }
}
