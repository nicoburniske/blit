pub mod drag;
pub mod focus;
pub mod press;
pub mod scroll;

pub fn press() -> press::Interact {
    press::Interact
}

pub fn drag() -> drag::Interact {
    drag::Interact::new()
}

pub fn focus() -> focus::Interact {
    focus::Interact
}

pub fn scroll() -> scroll::Interact {
    scroll::Interact::new()
}
