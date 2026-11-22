use blit::{Ui, Widget, state};

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

impl<C> Widget<C, state::Insert> for Tint
where
    C: Context,
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, C, state::Insert>) {
        ui.paint(move |cx, area| {
            cx.context_mut()
                .as_mut()
                .renderer
                .cells(area)
                .tint(self.color, self.opacity);
        });
    }
}
