mod desktop_demo;

fn main() {
    desktop_demo::run(
        blit_text_parley::Backend::new(),
        &[
            blit_gui::Family::named("Noto Color Emoji"),
            blit_gui::Family::named("Apple Color Emoji"),
        ],
    );
}
