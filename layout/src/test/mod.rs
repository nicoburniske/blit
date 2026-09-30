mod gui {
    type Length = f32;
    const TUI: bool = false;
    include!("cases.rs");
}

mod tui {
    type Length = u16;
    const TUI: bool = true;
    include!("cases.rs");
}
