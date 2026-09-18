pub mod performance;
pub use performance::Performance;

pub mod text_input;

pub use blit_std::widget::{popover, resize, scroll, split};
pub use text_input::TextInput;

pub use crate::text::Span;
use crate::{
    GuiContext, Ui, atom,
    color::Color,
    text::{TextOptions, TextStyle},
};
use blit::{Content, state};

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

impl Content<GuiContext> for Text<'_> {
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

impl Content<GuiContext> for RichText<'_> {
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
