mod desktop_demo;

fn main() {
    desktop_demo::run(blit_text_cosmic::Backend::new());
}
