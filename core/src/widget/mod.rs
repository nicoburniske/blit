//! frame-local widgets declare nodes through [`Widget::render`]

mod image;
mod rectangle;
mod scroll_area;
mod text;
mod text_input;

pub use image::Image;
pub use rectangle::Rectangle;
pub use scroll_area::{ScrollArea, ScrollScope, ScrollState};
pub use text::Text;
pub use text_input::{TextInput, TextInputResponse, TextInputState};

use crate::Ui;

pub trait Widget {
    type Output;

    fn render(self, ui: &mut Ui) -> Self::Output;
}

impl<F, R> Widget for F
where
    F: FnOnce(&mut Ui) -> R,
{
    type Output = R;

    fn render(self, ui: &mut Ui) -> Self::Output {
        self(ui)
    }
}
