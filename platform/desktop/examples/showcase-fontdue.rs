mod showcase_app;

fn main() {
    showcase_app::run(Box::new(blit_text_fontdue::Backend::new()));
}
