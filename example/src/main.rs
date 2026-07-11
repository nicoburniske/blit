mod platform;
mod todo;

use blit::{Input, Runtime};
use platform::TestPlatform;
use std::time::Instant;
use todo::TodoApp;

fn main() {
    let mut platform = TestPlatform::new(760, 560);
    let mut runtime = Runtime::new(platform.handle());
    let mut app = TodoApp::default();
    let started_at = Instant::now();
    while platform.is_open() {
        let input = platform.input();
        if input != Input::None || runtime.has_pending_redraw() {
            let screen = runtime.screen();
            runtime.render(started_at.elapsed(), input, |ui| app.render(ui, screen));
        }
        platform.present();
    }
}
