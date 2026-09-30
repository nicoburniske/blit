mod gui {
    type Length = f32;
    const TUI: bool = false;
    include!("cases/layouts.rs");
}

mod tui {
    type Length = u16;
    const TUI: bool = true;
    include!("cases/layouts.rs");
}
