mod showcase_app;

use blit_cpu::TextSystem;

fn main() {
    showcase_app::run(TextSystem::new(blit_text_fontdue::Backend::new()));
}
