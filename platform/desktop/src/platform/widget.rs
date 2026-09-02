pub use blit_std::widget::{scroll, split};

use blit::Widget;
use blit_cpu::{
    color::Color,
    text_types::{TextOptions, TextStyle},
};

use super::{DesktopPlatform, atom};
use crate::Ui;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Text<'a> {
        new(text: &'a str),
        style: TextStyle = TextStyle::default(),
        color: Color = Color::BLACK,
        offset_x: f32 = 0.0,
        options: TextOptions = TextOptions::default(),
    }
}

impl<S> Widget<DesktopPlatform, S> for Text<'_> {
    type Response = ();

    fn build(self, mut ui: Ui<'_, S>) {
        let run = ui.platform().text_run(self.text, self.style);
        ui.atom(
            atom::Text::new(run)
                .color(self.color)
                .offset_x(self.offset_x)
                .options(self.options),
        );
    }
}
