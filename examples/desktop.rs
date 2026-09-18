mod desktop_demo;

fn main() {
    desktop_demo::run(Box::new(blit_text_cosmic::Backend::new()));
}
