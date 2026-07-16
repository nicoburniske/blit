#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyboardKind {
    #[default]
    Alphanumeric,
    Password,
    Numbers,
    Decimal,
    Email,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardRequest<'a> {
    pub kind: KeyboardKind,
    pub request_caps: bool,
    pub accept_button_text: &'a str,
    pub accept_button_enabled: bool,
    pub delete_button_enabled: bool,
}
