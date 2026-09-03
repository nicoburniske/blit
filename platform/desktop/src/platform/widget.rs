pub use blit_std::widget::{popover, resize, scroll, split};

use blit::{Content, state};
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

impl Content<DesktopPlatform> for Text<'_> {
    type Response = ();

    fn append(self, mut ui: Ui<'_, state::Node>) {
        let run = ui.platform().text_run(self.text, self.style);
        ui.insert(
            atom::Text::new(run)
                .color(self.color)
                .offset_x(self.offset_x)
                .options(self.options),
        );
    }
}
