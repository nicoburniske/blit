use blit::{Ui, Widget};
use blit_cpu::{
    color::Color,
    text_types::{TextOptions, TextStyle},
};

use super::{DesktopPlatform, draw::TextRun};

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

impl Widget<DesktopPlatform> for Text<'_> {
    type Response = ();

    fn build(self, ui: &mut Ui<'_, DesktopPlatform>) {
        let run = ui.platform().text_run(self.text, self.style);
        ui.add(
            TextRun::new(run, self.style)
                .color(self.color)
                .offset_x(self.offset_x)
                .options(self.options),
        );
    }
}
