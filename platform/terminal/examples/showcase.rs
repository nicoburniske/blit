use std::io;

use blit::{
    Ui,
    input::{Input, Key},
};
use blit_showcase::Showcase;
use blit_terminal::ControlFlow;

fn main() -> io::Result<()> {
    let mut showcase = Showcase::default();
    blit_terminal::run(|ui: &mut Ui| {
        let control = if matches!(ui.input(), Input::Text('q'))
            || matches!(ui.input(), Input::Key(key) if key.key == Key::Escape)
        {
            ControlFlow::Exit
        } else {
            ControlFlow::Continue
        };
        showcase.render(ui);
        control
    })
}
