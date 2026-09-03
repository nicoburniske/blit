use blit::{Content, state};
use blit_tui_render::color::Color;

use crate::{TuiPlatform, Ui, atom};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Gauge<'a> {
        new(ratio: f64),
        @optional {
            label: &'a str,
        },
        filled: Color = Color::GREEN,
        unfilled: Color = Color::Reset,
    }
}

impl Content<TuiPlatform> for Gauge<'_> {
    type Response = ();

    fn append(self, mut ui: Ui<'_, state::Node>) {
        let mut gauge = atom::Gauge::new(self.ratio)
            .filled(self.filled)
            .unfilled(self.unfilled);
        if let Some(label) = self.label {
            gauge = gauge.label(ui.platform().renderer_mut().text_run(label));
        }
        ui.insert(gauge);
    }
}
