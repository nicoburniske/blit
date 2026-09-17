pub mod performance;
pub use performance::Performance;

mod polyline;
pub use polyline::Polyline;

pub mod text_input;

pub use blit_std::widget::{popover, resize, scroll, split};
pub use text_input::TextInput;

use blit::{Content, state};
use blit_cpu::{
    color::Color,
    text_types::{TextOptions, TextStyle},
};

use crate::{DesktopPlatform, Ui, atom};
pub use blit_cpu::text_types::Span;

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

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct RichText<'a> {
        new(spans: &'a [Span<'a>]),
        style: TextStyle = TextStyle::default(),
        color: Color = Color::BLACK,
        offset_x: f32 = 0.0,
        options: TextOptions = TextOptions::default(),
    }
}

impl Content<DesktopPlatform> for RichText<'_> {
    type Response = ();

    fn append(self, mut ui: Ui<'_, state::Node>) {
        let (run, palette) = ui.platform().rich_text(self.spans, self.style);
        ui.insert(
            atom::Text::new(run)
                .palette(palette)
                .color(self.color)
                .offset_x(self.offset_x)
                .options(self.options),
        );
    }
}
