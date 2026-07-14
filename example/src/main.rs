mod platform;
mod todo;

use blit::{Input, Runtime};
use platform::TestPlatform;
use std::time::Instant;
use todo::TodoApp;

fn main() {
    let mut runtime = Runtime::new(TestPlatform::new(760, 560));
    let mut app = TodoApp::default();
    let started_at = Instant::now();
    while runtime.platform().is_open() {
        let input = runtime.platform().input();
        let time = started_at.elapsed();
        let timer_due = runtime
            .next_timer_deadline()
            .is_some_and(|deadline| time >= deadline);
        if input != Input::None || runtime.has_pending_redraw() || timer_due {
            let screen = runtime.screen();
            runtime.render(time, input, |ui| app.render(ui, screen));
        }
        runtime.platform().present();
    }
}
