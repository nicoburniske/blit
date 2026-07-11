mod platform;
mod todo;

use bullseye::{Input, Runtime};
use platform::TestPlatform;
use todo::TodoApp;

fn main() {
    let mut platform = TestPlatform::new(760, 560);
    let mut runtime = Runtime::new(platform.handle());
    let mut app = TodoApp::default();
    while platform.is_open() {
        let input = platform.input();
        if input != Input::None || runtime.has_pending_redraw() {
            let screen = runtime.screen();
            runtime.render(input, |ui| app.render(ui, screen));
        }
        platform.present();
    }
}
