pub mod drag;
pub mod focus;
pub mod keyboard;
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

/// handles routed keyboard and text input, claiming it when `handler` returns `Some`
///
/// return an action and apply it during the build.
/// use [`keyboard::Interact::require_focus`] to require this widget to have focus.
pub fn keyboard<F: keyboard::Handler>(handler: F) -> keyboard::Interact<F> {
    keyboard::Interact {
        handler,
        require_focus: false,
    }
}

pub fn scroll() -> scroll::Interact {
    scroll::Interact::new()
}
